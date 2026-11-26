import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Backend (Rust/axum) during `pnpm dev`. In production the binary serves the SPA itself.
const backend = 'http://127.0.0.1:8080';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				// Pure SPA: every unknown path (including /snap/<token>) is served by index.html.
				fallback: 'index.html'
			})
		})
	],
	server: {
		proxy: {
			'/admin': backend,
			'/snap/api': backend,
			'/v1': backend,
			'/healthz': backend
		}
	}
});
