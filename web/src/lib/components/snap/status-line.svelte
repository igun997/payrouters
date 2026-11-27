<script lang="ts">
	/* Waiting-for-payment line. Sticky to the bottom on phones, in flow from 640. */
	let { title, hint }: { title: string; hint: string } = $props();
</script>

<div
	data-testid="snap-status"
	data-status="pending"
	role="status"
	aria-live="polite"
	class="sticky bottom-0 z-10 -mx-page flex items-center gap-3 border-t border-border bg-background px-page pt-3 pb-[max(12px,env(safe-area-inset-bottom))] sm:static sm:mx-0 sm:border-0 sm:bg-transparent sm:px-0 sm:pt-0 sm:pb-0"
>
	<span class="relative flex size-3 shrink-0" aria-hidden="true">
		<span
			class="absolute inline-flex h-full w-full animate-ping rounded-full bg-pending-dot opacity-60 motion-reduce:hidden"
		></span>
		<span class="relative inline-flex size-3 rounded-full bg-pending-dot"></span>
	</span>
	<span class="flex flex-col gap-0.5">
		<span class="text-label font-semibold">{title}</span>
		<span class="text-helper text-muted-foreground">{hint}</span>
	</span>
</div>
