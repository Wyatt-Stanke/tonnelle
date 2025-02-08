import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import UnoCSS from "unocss/vite";

// https://vite.dev/config/
export default defineConfig({
	plugins: [react(), UnoCSS()],
	server: {
		cors: {
			origin: "http://129.80.117.126",
		},
	},
});
