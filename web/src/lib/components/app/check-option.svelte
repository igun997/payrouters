<script lang="ts">
	/**
	 * One selectable option as a card with a native checkbox underneath (keyboard and screen readers
	 * work unchanged). Styled like `RadioCards`; the ui kit has no checkbox.
	 */
	import Check from '@lucide/svelte/icons/check';
	import { cn } from '#lib/utils';

	let {
		label,
		helper,
		checked,
		disabled = false,
		testid,
		onchange
	}: {
		label: string;
		helper?: string;
		checked: boolean;
		disabled?: boolean;
		testid?: string;
		onchange: (checked: boolean) => void;
	} = $props();
</script>

<label
	data-testid={testid}
	class={cn(
		'flex min-h-touch cursor-pointer items-start gap-3 rounded-md border border-input bg-card p-3.5 transition-colors',
		'has-checked:border-primary has-checked:ring-1 has-checked:ring-primary',
		'has-focus-visible:ring-3 has-focus-visible:ring-ring/50',
		disabled && 'cursor-not-allowed opacity-60'
	)}
>
	<input
		type="checkbox"
		class="peer sr-only"
		{checked}
		{disabled}
		onchange={(event) => onchange(event.currentTarget.checked)}
	/>
	<span
		aria-hidden="true"
		class="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-sm border-[1.5px] border-input text-primary-foreground peer-checked:border-primary peer-checked:bg-primary [&>svg]:opacity-0 peer-checked:[&>svg]:opacity-100"
	>
		<Check class="size-3" />
	</span>
	<span class="flex min-w-0 flex-col gap-0.5">
		<span class="text-label break-words text-foreground">{label}</span>
		{#if helper}
			<span class="text-helper break-words text-muted-foreground">{helper}</span>
		{/if}
	</span>
</label>
