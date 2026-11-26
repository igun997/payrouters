	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '#lib/components/ui/sonner';
	<title>PayRouters</title>
<ModeWatcher defaultMode="system" />
<!-- Toasts sit bottom-centre; below md they clear the 88px phone tab bar (--toaster-bottom). -->
<Toaster
	richColors
	closeButton
	position="bottom-center"
	offset={{ bottom: 'var(--toaster-bottom)' }}
	mobileOffset={{ bottom: 'var(--toaster-bottom)' }}
/>

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
