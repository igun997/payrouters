<script lang="ts">
	import './layout.css';
	import favicon from '#lib/assets/favicon.svg';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '#lib/components/ui/sonner';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<title>PayRouters</title>
	<link rel="icon" href={favicon} />
</svelte:head>

<ModeWatcher defaultMode="system" />
<!-- Toasts sit bottom-centre; below md they clear the 88px phone tab bar (--toaster-bottom). -->
<Toaster
	richColors
	closeButton
	position="bottom-center"
	offset={{ bottom: 'var(--toaster-bottom)' }}
	mobileOffset={{ bottom: 'var(--toaster-bottom)' }}
/>

{@render children()}

<style>
	:global(:root) {
		--toaster-bottom: 24px;
	}
	@media (max-width: 767px) {
		:global(:root) {
			--toaster-bottom: calc(96px + env(safe-area-inset-bottom));
		}
	}
</style>
