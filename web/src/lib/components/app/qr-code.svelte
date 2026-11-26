<script lang="ts">
	import QRCode from 'qrcode';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import { cn } from '#lib/utils';

	/**
	 * Renders `text` as a QR code image. `size` is the rendered pixel size; `class` may override the
	 * displayed width (the image keeps its pixels and scales). `src` is bound to the generated PNG data
	 * URL so a parent can offer it as a download.
	 */
	let {
		text,
		size = 240,
		alt = 'QR code',
		src = $bindable(null),
		class: className
	}: {
		text: string;
		size?: number;
		alt?: string;
		src?: string | null;
		class?: string;
	} = $props();

	let failed = $state(false);

	$effect(() => {
		const value = text;
		let cancelled = false;
		src = null;
		failed = false;
		QRCode.toDataURL(value, { margin: 1, width: size, errorCorrectionLevel: 'M' })
			.then((url) => {
				if (!cancelled) src = url;
			})
			.catch(() => {
				if (!cancelled) failed = true;
			});
		return () => {
			cancelled = true;
		};
	});
</script>

{#if src}
	<!-- QR codes need a light background to scan, regardless of the app theme. -->
	<img
		{src}
		{alt}
		width={size}
		height={size}
		class={cn('mx-auto h-auto max-w-full rounded-md bg-white p-2', className)}
	/>
{:else if failed}
	<p class="text-center text-sm text-destructive">Could not render the QR code.</p>
{:else}
	<Skeleton class="mx-auto" style="width: min({size}px, 100%); aspect-ratio: 1" />
{/if}
