<script lang="ts">
	/*
	 * Bank virtual account. For IDRX (payable_amount above amount) the breakdown and the exact-amount
	 * warning replace the generic notice and the how-to-pay list, as in the design.
	 */
	import Banner from '#lib/components/app/banner.svelte';
	import { Separator } from '#lib/components/ui/separator';
	import BankTile from './bank-tile.svelte';
	import BreakdownCard from './breakdown-card.svelte';
	import CopyAction from './copy-action.svelte';
	import HowToPay from './how-to-pay.svelte';
	import { vaGuides } from './how-to-pay-data';
	import { bankBadge, bankName } from './method-meta';
	import type { Instructions, SnapPayment } from '#lib/api/types';
	import { formatIdr } from '#lib/format';

	type VirtualAccount = Extract<Instructions, { type: 'virtual_account' }>;

	let {
		payment,
		instructions,
		breakdown
	}: { payment: SnapPayment; instructions: VirtualAccount; breakdown: boolean } = $props();

	const account = $derived(instructions.account_number ?? '');
	// Groups of four for reading. The digits are one text run so the number can be copied exactly.
	const groups = $derived(account.match(/.{1,4}/g) ?? []);
	const total = $derived(payment.payable_amount ?? payment.amount);
	const fee = $derived(total - payment.amount);
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="snap-va"
>
	<div class="flex items-center gap-3">
		<BankTile text={bankBadge(instructions.bank)} code={instructions.bank} />
		<div class="flex min-w-0 flex-col">
			<p class="text-card-title font-semibold">{bankName(instructions.bank)}</p>
			<p class="text-helper text-muted-foreground">Bank transfer</p>
		</div>
	</div>
	<Separator />
	<div class="flex flex-col gap-2">
		<span class="text-helper text-muted-foreground">Account number</span>
		<p
			data-testid="snap-account-number"
			class="flex flex-wrap gap-x-2.5 text-mono-va font-mono font-semibold text-foreground"
		>
			{#each groups as group, index (index)}<span class="whitespace-nowrap">{group}</span>{/each}
		</p>
		<CopyAction
			text={account}
			label="Copy account number"
			copiedLabel="Copied"
			toastMessage="Account number copied"
			variant="default"
			size="lg"
			class="w-full"
			testid="snap-copy-account"
		/>
	</div>
	{#if instructions.account_name}
		<div class="flex items-center justify-between gap-3">
			<span class="text-helper text-muted-foreground">Account name</span>
			<span class="text-right text-label font-semibold">{instructions.account_name}</span>
		</div>
	{/if}
</section>

{#if breakdown}
	<BreakdownCard
		title="Breakdown"
		rows={[
			{ label: 'IDRX to mint', value: formatIdr(payment.amount) },
			{ label: 'VA fee', value: formatIdr(fee) }
		]}
		total={{ label: 'Total to pay', value: formatIdr(total) }}
	/>
	<Banner tone="warn" title={`Pay exactly ${formatIdr(total)}`}>
		Paying a different amount holds this mint for manual review. Underpaid and overpaid transfers
		are not minted automatically.
	</Banner>
{:else}
	<Banner tone="info">
		Transfer the exact amount above. The payment is confirmed automatically once the bank receives
		it.
	</Banner>
	<HowToPay guides={vaGuides(instructions.bank)} />
{/if}
