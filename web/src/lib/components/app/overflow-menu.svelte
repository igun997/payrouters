<script lang="ts">
	/** "More actions" menu for a row. Items run on select; `destructive` items are red. */
	import type { Component } from 'svelte';
	import { DropdownMenu } from 'bits-ui';
	import { Button } from '#lib/components/ui/button';
	import Ellipsis from '@lucide/svelte/icons/ellipsis';
	import { cn } from '#lib/utils';

	export type MenuItem = {
		label: string;
		icon?: Component<{ class?: string }>;
		destructive?: boolean;
		onselect: () => void;
	};

	let { label, items, testid }: { label: string; items: MenuItem[]; testid?: string } = $props();
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<Button {...props} variant="outline" size="icon" aria-label={label} data-testid={testid}>
				<Ellipsis />
			</Button>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Portal>
		<DropdownMenu.Content
			align="end"
			sideOffset={6}
			class="z-50 min-w-52 rounded-md border bg-popover p-1.5 text-popover-foreground shadow-md"
		>
			{#each items as item (item.label)}
				{@const Icon = item.icon}
				<DropdownMenu.Item
					class={cn(
						'flex min-h-touch cursor-pointer items-center gap-2.5 rounded-sm px-2.5 text-body outline-none data-highlighted:bg-muted',
						item.destructive ? 'text-destructive' : 'text-foreground'
					)}
					onSelect={item.onselect}
				>
					{#if Icon}<Icon class="size-4 shrink-0" />{/if}
					{item.label}
				</DropdownMenu.Item>
			{/each}
		</DropdownMenu.Content>
	</DropdownMenu.Portal>
</DropdownMenu.Root>
