<script lang="ts">
	/* Order id and description under the amount. The deadline is optional: pages that show it elsewhere omit it. */
	import Receipt from '@lucide/svelte/icons/receipt';

	let { orderId, description = null }: { orderId: string; description?: string | null } = $props();
</script>

<p class="flex flex-wrap items-center gap-x-2 gap-y-1 text-helper text-muted-foreground">
	<span class="inline-flex items-center gap-1.5">
		<Receipt class="size-3.5 shrink-0" aria-hidden="true" />
		<span class="font-mono text-foreground">{orderId}</span>
	</span>
	{#if description}
		<span aria-hidden="true">·</span>
		<span>{description}</span>
	{/if}
</p>
