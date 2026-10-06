use crate::diagnostics::sampler::{
    descendant_pids, MachineHardware, ProcessHistory, ProcessRecord,
};
use std::collections::HashSet;
use std::mem::size_of;
use std::time::Instant;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::SystemInformation::{
    GlobalMemoryStatusEx, MEMORYSTATUSEX, OSVERSIONINFOW,
};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetCurrentProcess, GetCurrentProcessId, GetProcessHandleCount,
    GetProcessTimes, OpenProcess, ALL_PROCESSOR_GROUPS, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

#[link(name = "ntdll")]
extern "system" {
    fn RtlGetVersion(version: *mut OSVERSIONINFOW) -> i32;
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this handle is owned by ProcessHandle and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub(super) fn processes(history: &mut ProcessHistory) -> Vec<ProcessRecord> {
    let now = Instant::now();
    // SAFETY: the process snapshot call takes scalar arguments and returns an owned handle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = ProcessHandle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut records = Vec::new();
    // SAFETY: entry has the required size and remains valid for each ToolHelp call.
    let mut has_entry = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while has_entry {
        let name = String::from_utf16_lossy(&entry.szExeFile)
            .trim_end_matches('\0')
            .to_string();
        records.push(ProcessRecord {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            name,
            working_set_bytes: 0,
            private_bytes: 0,
            cpu_percent: 0.0,
            handle_count: None,
            thread_count: u64::from(entry.cntThreads),
        });
        // SAFETY: entry is initialized and points to writable storage for the next row.
        has_entry = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    // SAFETY: the current process ID query takes no pointers or handles.
    let self_pid = unsafe { GetCurrentProcessId() };
    let descendants = descendant_pids(&records, &[self_pid]);
    records.retain_mut(|record| {
        if record.pid != self_pid && !descendants.contains(&record.pid) {
            return false;
        }
        let (working_set_bytes, private_bytes, cpu_ticks) = process_usage(record.pid);
        record.working_set_bytes = working_set_bytes;
        record.private_bytes = private_bytes;
        record.cpu_percent = history.cpu_percent(record.pid, cpu_ticks, now);
        if record.pid == self_pid {
            record.handle_count = current_handle_count();
        }
        history.observe(record.pid, cpu_ticks);
        true
    });
    history.finish(
        records
            .iter()
            .map(|record| record.pid)
            .collect::<HashSet<_>>(),
        now,
    );
    records
}

pub(super) fn hardware() -> MachineHardware {
    // SAFETY: the processor count query takes only a scalar processor-group value.
    let logical_cores = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    let mut memory = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: memory has its length field initialized and valid output storage.
    let total_ram_bytes = if unsafe { GlobalMemoryStatusEx(&mut memory) } != 0 {
        memory.ullTotalPhys
    } else {
        0
    };
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: version has the required size and valid output storage.
    let windows_build = if unsafe { RtlGetVersion(&mut version) } == 0 {
        Some(u64::from(version.dwBuildNumber))
    } else {
        None
    };
    MachineHardware {
        logical_cores: u64::from(logical_cores.max(1)),
        total_ram_bytes,
        system_drive_type: system_drive_type(),
        windows_build,
    }
}

fn system_drive_type() -> &'static str {
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
    const STORAGE_DEVICE_SEEK_PENALTY_PROPERTY: u32 = 7;
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        return "unknown";
    };
    let system_root = system_root.to_string_lossy();
    let Some(drive) = system_root.get(..2).filter(|drive| drive.ends_with(':')) else {
        return "unknown";
    };
    let device = format!(r"\\.\{drive}");
    let wide: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: wide is a NUL-terminated device name and all optional handles are null.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return "unknown";
    }
    let handle = ProcessHandle(handle);
    let mut query = [0_u8; 12];
    query[..4].copy_from_slice(&STORAGE_DEVICE_SEEK_PENALTY_PROPERTY.to_le_bytes());
    let mut output = [0_u8; 12];
    let mut returned = 0;
    // SAFETY: query and output are valid fixed-size buffers for the storage property request.
    let result = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            query.as_ptr().cast(),
            query.len() as u32,
            output.as_mut_ptr().cast(),
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if result == 0 || returned < 9 {
        return "unknown";
    }
    match output[8] {
        0 => "ssd",
        1 => "hdd",
        _ => "unknown",
    }
}

pub(super) fn ticks_per_second() -> f64 {
    10_000_000.0
}

pub(super) fn current_handle_count() -> Option<u64> {
    let mut count = 0;
    // SAFETY: GetCurrentProcess returns a pseudo-handle owned by the operating system.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: count points to writable storage and process is the current-process pseudo-handle.
    (unsafe { GetProcessHandleCount(process, &mut count) } != 0).then_some(u64::from(count))
}

fn process_usage(pid: u32) -> (u64, u64, u64) {
    let access = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ;
    // SAFETY: the request opens only the process identified by ToolHelp for read-only queries.
    let handle = unsafe { OpenProcess(access, 0, pid) };
    if handle.is_null() {
        return (0, 0, 0);
    }
    let handle = ProcessHandle(handle);
    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: the extended counters begin with the base counters and cb specifies their size.
    let memory_ok =
        unsafe { GetProcessMemoryInfo(handle.0, &mut memory as *mut _ as *mut _, memory.cb) } != 0;
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: each FILETIME pointer refers to valid writable storage.
    let times_ok =
        unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) } != 0;
    let working = if memory_ok {
        memory.WorkingSetSize as u64
    } else {
        0
    };
    let private = if memory_ok {
        memory.PrivateUsage as u64
    } else {
        0
    };
    let cpu = if times_ok {
        filetime(&kernel).saturating_add(filetime(&user))
    } else {
        0
    };
    (working, private, cpu)
}

fn filetime(value: &FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_combines_high_and_low_parts() {
        let value = FILETIME {
            dwLowDateTime: 17,
            dwHighDateTime: 2,
        };
        assert_eq!(filetime(&value), (2_u64 << 32) | 17);
    }

    #[test]
    fn current_process_is_the_sampler_root() {
        // SAFETY: the current process ID query takes no pointers or handles.
        assert_ne!(unsafe { GetCurrentProcessId() }, 0);
        assert!(current_handle_count().is_some());
    }
}
