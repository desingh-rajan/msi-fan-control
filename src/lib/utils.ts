/** Convert bytes to a GB string, rounded to 1 decimal place. */
export function formatGb(bytes: number): string {
  return (bytes / 1024 / 1024 / 1024).toFixed(1);
}
