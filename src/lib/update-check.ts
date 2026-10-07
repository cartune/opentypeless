/**
 * Update checks are skipped in Vite dev mode (unsigned local builds would
 * otherwise be offered an "update" from the release feed) and whenever
 * VITE_DISABLE_UPDATE_CHECK=1 is set at build time.
 */
export function shouldCheckForUpdates(
  env: { DEV?: boolean; VITE_DISABLE_UPDATE_CHECK?: string } = import.meta.env,
): boolean {
  if (env.DEV) return false
  return env.VITE_DISABLE_UPDATE_CHECK !== '1'
}
