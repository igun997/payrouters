<script lang="ts">
	import type { Snippet } from 'svelte';

	// Customer-facing routes (no admin navigation). Each page draws its own shell.
	let { children }: { children: Snippet } = $props();
</script>

<div class="flex min-h-svh flex-col bg-background text-foreground">
	{@render children()}
</div>
