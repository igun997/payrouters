<script lang="ts">
	import * as Select from '#lib/components/ui/select';
	import { cn } from '#lib/utils';

	type Option = { value: string; label: string };

	let {
		value = $bindable(''),
		options,
		label,
		class: className
	}: { value?: string; options: Option[]; label: string; class?: string } = $props();

	const selectedLabel = $derived(options.find((o) => o.value === value)?.label ?? label);
</script>

<Select.Root type="single" bind:value>
	<Select.Trigger class={cn('w-44', className)} aria-label={label}>{selectedLabel}</Select.Trigger>
	<Select.Content>
		<Select.Group>
			{#each options as option (option.value)}
				<Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
			{/each}
		</Select.Group>
	</Select.Content>
</Select.Root>
