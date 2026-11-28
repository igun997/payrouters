<script lang="ts">
	/**
	 * A product's payment options at a glance: one line per provider with what it serves as chips
	 * ("QRIS", "BCA VA", "Hosted checkout"). Nothing set reads as a single-provider product and names
	 * the default provider, so it is clear what happens to a payment that names no provider.
	 */
	import type { PaymentOption } from '#lib/api/types';
	import Pill from '#lib/components/app/pill.svelte';
	import { groupPaymentOptions } from '#lib/payment-options';

	let {
		options,
		providers,
		methods,
		defaultLabel
	}: {
		options: PaymentOption[];
		/** Provider id → label. */
		providers: Record<string, string>;
		/** Method code → label. */
		methods: Record<string, string>;
		/** Label of the product's default provider, or "No default". */
		defaultLabel: string;
	} = $props();

	const groups = $derived(groupPaymentOptions(options, providers, methods));
</script>

{#if groups.length === 0}
	<div class="flex min-w-0 flex-col gap-1.5" data-testid="payment-options-single">
		<span class="text-body text-foreground">Single provider</span>
		<span class="text-helper text-muted-foreground">
			Payments use the provider in the request, else the default ({defaultLabel}). Set options to
			offer several providers on one product.
		</span>
	</div>
{:else}
	<ul class="flex min-w-0 flex-col gap-2.5" data-testid="payment-options-groups">
		{#each groups as group (group.provider)}
			<li
				class="flex min-w-0 flex-col gap-1 sm:flex-row sm:items-baseline sm:gap-3"
				data-testid="payment-options-group"
				data-provider={group.provider}
			>
				<span class="text-label break-words text-foreground sm:w-20 sm:shrink-0">
					{group.label}
				</span>
				<span class="flex min-w-0 flex-wrap gap-1.5">
					{#each group.items as item (item.key)}
						<Pill tone="neutral">{item.label}</Pill>
					{/each}
				</span>
			</li>
		{/each}
	</ul>
{/if}
