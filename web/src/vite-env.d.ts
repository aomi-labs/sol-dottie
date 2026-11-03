/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_AOMI_API_URL?: string;
  readonly VITE_DOTTIE_APPLICATION_ID?: string;
  readonly VITE_DOTTIE_APP_ID?: string;
  readonly VITE_DOTTIE_DEFAULT_MODEL?: string;
  readonly VITE_DOTTIE_SHOW_MODEL?: string;
  readonly VITE_DOTTIE_TELEGRAM_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
