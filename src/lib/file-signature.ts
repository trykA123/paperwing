import type { CompareFile } from './api';

/** Equal for two fetches of the same file entry, so a refreshed file list does not look like a new file. */
export function fileSignature(file: CompareFile | undefined): string {
  return file ? JSON.stringify([file.id, file.path, file.left, file.right]) : '';
}
