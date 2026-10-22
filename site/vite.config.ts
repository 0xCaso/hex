import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

export default defineConfig({
  plugins: [react()],
  // PROTOTYPE (site/voice-game): served over tailnet HTTPS for the microphone.
  server: { allowedHosts: [".ts.net"] },
  optimizeDeps: { exclude: ["@huggingface/transformers"] },
  worker: { format: "es" },
})
