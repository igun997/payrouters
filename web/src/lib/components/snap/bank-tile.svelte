<script lang="ts">
	/*
	 * Square tile for a bank or QRIS. Shows the logo the server fetched (GET /assets/logo/<code>) and
	 * keeps the short code (BCA, BNI, MDR ...) as the fallback while it loads, when the bank has no
	 * logo, or when the fetch failed. The code stays in the DOM for assistive technology and tests.
	 */
	import { cn } from '#lib/utils';
	import { logoCode } from './method-meta';

	let { text, code, class: className }: { text: string; code?: string; class?: string } = $props();

	const logo = $derived(code ? logoCode(code) : null);
	let loaded = $state(false);
	let failed = $state(false);

	$effect(() => {
		void logo;
		loaded = false;
		failed = false;
	});
</script>

<span
	class={cn(
		'relative flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-md bg-secondary text-label font-semibold text-foreground',
		className
	)}
	aria-hidden="true"
>
	<span class={loaded ? 'sr-only' : undefined}>{text}</span>
	{#if logo && !failed}
		<img
			src={`/assets/logo/${logo}`}
			alt=""
			width="44"
			height="44"
			loading="lazy"
			decoding="async"
			class={cn(
				'absolute inset-0 size-full bg-white object-contain p-1.5 transition-opacity duration-150',
				loaded ? 'opacity-100' : 'opacity-0'
			)}
			onload={() => (loaded = true)}
			onerror={() => (failed = true)}
		/>
	{/if}
</span>
