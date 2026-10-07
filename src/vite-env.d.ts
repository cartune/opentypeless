/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_VERSION?: string
  readonly VITE_API_BASE_URL?: string
  readonly VITE_DISABLE_UPDATE_CHECK?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
