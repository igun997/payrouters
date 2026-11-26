<script lang="ts">
	import { cn } from '#lib/utils';

	/**
	 * Loading placeholder: a secondary-colour block with a soft pulse (off for reduced motion). No
	 * spinners. Size and shape come from `class`, e.g. `h-4 w-40` for a text line.
	 */
	let { class: className }: { class?: string } = $props();
</script>

<div
	aria-hidden="true"
	data-testid="skeleton-block"
	class={cn('rounded-md bg-secondary motion-safe:animate-pulse', className)}
></div>
