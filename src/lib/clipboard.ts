/**
 * Copy text for the user. The native clipboard plugin is the reliable path in
 * the Tauri webview (WKWebView rejects `navigator.clipboard` outside a focused,
 * trusted gesture); the web API is only a fallback for browsers and tests.
 */
export async function copyTextToClipboard(text: string): Promise<void> {
  try {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager')
    await writeText(text)
    return
  } catch (error) {
    console.warn('Native clipboard write failed, trying the web clipboard:', error)
  }
  await navigator.clipboard.writeText(text)
}
