<script lang="ts">
	/* Mandiri Multipayment: matched by biller code and bill key, not by an account number. */
	import Banner from '#lib/components/app/banner.svelte';
	import { Separator } from '#lib/components/ui/separator';
	import BankTile from './bank-tile.svelte';
	import CopyRow from './copy-row.svelte';
	import HowToPay from './how-to-pay.svelte';
	import { mandiriBillGuides } from './how-to-pay-data';
	import type { Instructions } from '#lib/api/types';
	import { formatIdr } from '#lib/format';

	type BillInstructions = Extract<Instructions, { type: 'virtual_account' }>;

	let { total, instructions }: { total: number; instructions: BillInstructions } = $props();

	const amountText = $derived(formatIdr(total));
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="snap-mandiri-bill"
>
	<div class="flex items-center gap-3">
		<BankTile text="MDR" />
		<div class="flex min-w-0 flex-col">
			<p class="text-card-title font-semibold">Mandiri Bill Payment</p>
			<p class="text-helper text-muted-foreground">Bank transfer · Multipayment</p>
		</div>
	</div>
	<Separator />
	{#if instructions.biller_code}
		<CopyRow
			label="Biller code"
			value={instructions.biller_code}
			testid="snap-biller-code"
			ariaLabel="Copy biller code"
		/>
	{/if}
	{#if instructions.bill_key}
		<CopyRow
			label="Bill key"
			value={instructions.bill_key}
			testid="snap-bill-key"
			ariaLabel="Copy bill key"
		/>
	{/if}
	<p class="text-helper text-muted-foreground">
		Payment is matched by biller code and bill key, not by account number.
	</p>
</section>

<Banner tone="info">
	Pay exactly {amountText}. A different amount will not be matched to this order.
</Banner>
<HowToPay guides={mandiriBillGuides(amountText)} />
