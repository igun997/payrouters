<script lang="ts">
	// Two columns below xl, four from xl. Amounts never wrap: use fewer columns, not a smaller font.
	import type { Snippet } from 'svelte';

	let { children, testid }: { children: Snippet; testid?: string } = $props();
</script>

<div class="grid grid-cols-2 gap-section xl:grid-cols-4" data-testid={testid}>
	{@render children()}
</div>
