import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

export default defineConfig({
  plugins: [react()],
  // Previews are shared over the tailnet.
  server: { allowedHosts: [".ts.net"] },
  preview: { allowedHosts: [".ts.net"] },
})
