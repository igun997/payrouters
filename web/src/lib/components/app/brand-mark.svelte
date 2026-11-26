<script lang="ts">
	import { cn } from '#lib/utils';

	/** PayRouters mark: three lines merging into one route. `tile` adds the evergreen rounded square. */
	let {
		size = 22,
		tile = false,
		class: className
	}: { size?: number; tile?: boolean; class?: string } = $props();
</script>

{#if tile}
	<span
		class={cn(
			'inline-flex shrink-0 items-center justify-center rounded-[10px] bg-primary text-primary-foreground',
			className
		)}
		style={`width:${Math.round(size * 1.64)}px;height:${Math.round(size * 1.64)}px`}
	>
		<svg
			viewBox="0 0 24 24"
			width={size}
			height={size}
			fill="none"
			stroke="currentColor"
			stroke-width="2.2"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
		>
			<path d="M3 5h5c4.5 0 4.5 7 9 7h4" /><path d="M3 12h18" /><path d="M3 19h5c4.5 0 4.5-7 9-7" />
		</svg>
	</span>
{:else}
	<svg
		viewBox="0 0 24 24"
		width={size}
		height={size}
		fill="none"
		stroke="currentColor"
		stroke-width="2.2"
		stroke-linecap="round"
		stroke-linejoin="round"
		class={className}
		aria-hidden="true"
	>
		<path d="M3 5h5c4.5 0 4.5 7 9 7h4" /><path d="M3 12h18" /><path d="M3 19h5c4.5 0 4.5-7 9-7" />
	</svg>
{/if}
