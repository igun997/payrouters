<script lang="ts">
	import * as Select from '#lib/components/ui/select';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import { cn } from '#lib/utils';

	type Option = { value: string; label: string };

	let {
		value = $bindable(''),
		options,
		label,
		class: className,
		loading = false
	}: {
		value?: string;
		options: Option[];
		label: string;
		class?: string;
		/** Shows a placeholder while the options are still being fetched. */
		loading?: boolean;
	} = $props();

	const selectedLabel = $derived(options.find((o) => o.value === value)?.label ?? label);
</script>

{#if loading}
	<Skeleton class={cn('h-control w-44 rounded-md', className)} aria-label={`Loading ${label}`} />
{:else}
	<Select.Root type="single" bind:value>
		<Select.Trigger
			class={cn(
				'h-auto min-h-11 w-44 max-w-full text-left whitespace-normal data-placeholder:text-foreground',
				className
			)}
			aria-label={label}
		>
			{selectedLabel}
		</Select.Trigger>
		<Select.Content>
			<Select.Group>
				{#each options as option (option.value)}
					<Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
				{/each}
			</Select.Group>
		</Select.Content>
	</Select.Root>
{/if}
