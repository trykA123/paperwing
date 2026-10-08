import { expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture';
import { api } from './api';

test('CI reads pass targets and conditional page validators through IPC', async () => {
  const calls = [];
  const target = { path: 'C:\\repos\\admin', branch: 'main' };
  const query = { page: 2, etag: '"ci-fixture"', branch: 'main' };
  await withIpc((command, args) => {
    calls.push({ command, args });
    return { state: 'notModified', etag: query.etag };
  }, async () => {
    expect(await api.ciRuns(target, query)).toEqual({ state: 'notModified', etag: query.etag });
    await api.ciJobs(target, '31', query);
    await api.ciArtifacts(target, '31');
    await api.ciJobLog(target, '32');
    await api.ciDownloadArtifact(target, '33');
    await api.ciDownloadRunLogs(target, '31');
  });
  expect(calls).toEqual([
    { command: 'ci_runs', args: { target, query } },
    { command: 'ci_jobs', args: { target, runId: '31', query } },
    { command: 'ci_artifacts', args: { target, runId: '31', query: {} } },
    { command: 'ci_job_log', args: { target, jobId: '32' } },
    { command: 'ci_download_artifact', args: { target, artifactId: '33' } },
    { command: 'ci_download_run_logs', args: { target, runId: '31' } },
  ]);
});

test('CI writes preserve explicit confirmation and dispatch inputs', async () => {
  const calls = [];
  const target = { path: '/workspace/admin', branch: 'main' };
  const rerun = { runId: '31', failedOnly: true, confirmed: false };
  const cancel = { runId: '31', confirmed: true };
  const dispatch = { pipelineId: '7', reference: 'release/1', inputs: { dry: true } };
  await withIpc((command, args) => { calls.push({ command, args }); return null; }, async () => {
    await api.ciRerun(target, rerun);
    await api.ciCancel(target, cancel);
    await api.ciDispatchInputs(target, dispatch);
    await api.ciDispatch(target, { dispatch, confirmed: true });
  });
  expect(calls).toEqual([
    { command: 'ci_rerun', args: { target, request: rerun } },
    { command: 'ci_cancel', args: { target, request: cancel } },
    { command: 'ci_dispatch_inputs', args: { target, request: dispatch } },
    { command: 'ci_dispatch', args: { target, request: { dispatch, confirmed: true } } },
  ]);
});

test('CI wrappers preserve neutral rate limit errors', async () => {
  const error = { kind: 'rateLimited', resetAt: '2026-10-07T16:00:00+00:00' };
  await withIpc(() => { throw error; }, async () => {
    await expect(api.ciRuns({ path: '/workspace/admin', branch: 'main' })).rejects.toEqual(error);
  });
});
