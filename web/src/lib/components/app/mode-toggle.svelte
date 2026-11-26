<script lang="ts">
	// Sandbox / Live switch. Switching to Live asks first (confirm dialog on desktop, chooser sheet
	// on phones). `responsive` shows the segmented control from md and a chip below md; `segmented`
	// is always the control (used in the More panel).
	import * as ToggleGroup from '#lib/components/ui/toggle-group';
	import { Button } from '#lib/components/ui/button';
	import ResponsiveDialog from './responsive-dialog.svelte';
	import Banner from './banner.svelte';
	import { prefs } from '#lib/state/prefs.svelte';
	import type { Mode } from '#lib/api/types';
	import { cn } from '#lib/utils';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import Check from '@lucide/svelte/icons/check';

	let { variant = 'responsive' }: { variant?: 'responsive' | 'segmented' } = $props();

	let confirmOpen = $state(false);
	let chooserOpen = $state(false);
	let picked = $state<Mode>('sandbox');
	/** Selection shown by the segmented control. Clicking Live only opens the confirm; the group
	 * snaps back to the stored mode whenever the dialog opens or closes, or the mode changes. */
	let groupValue = $state<string>('sandbox');

	$effect(() => {
		void confirmOpen;
		groupValue = prefs.mode;
	});

	const options: { value: Mode; title: string; body: string }[] = [
		{
			value: 'sandbox',
			title: 'Sandbox',
			body: 'Test payments through sandbox accounts only.'
		},
		{ value: 'live', title: 'Live', body: 'Real payments through live provider accounts.' }
	];

	function choose(value: string) {
		if (value !== 'sandbox' && value !== 'live') return;
		if (value === prefs.mode) return;
		if (value === 'sandbox') {
			prefs.setMode('sandbox');
			return;
		}
		confirmOpen = true;
	}

	function goLive() {
		prefs.setMode('live');
		confirmOpen = false;
	}

	function openChooser() {
		picked = prefs.mode;
		chooserOpen = true;
	}

	function applyPicked() {
		prefs.setMode(picked);
		chooserOpen = false;
	}

	const chipTone = $derived(
		prefs.mode === 'live'
			? 'bg-success text-success-foreground'
			: 'bg-sandbox text-sandbox-foreground'
	);
</script>

{#snippet segmented(testid: string, full: boolean)}
	<ToggleGroup.Root
		type="single"
		variant="outline"
		size="sm"
		bind:value={groupValue}
		onValueChange={choose}
		aria-label="Data mode"
		data-testid={testid}
		class={cn('h-auto gap-1 rounded-full border-0 bg-secondary p-1 shadow-none', full && 'w-full')}
	>
		<ToggleGroup.Item
			value="sandbox"
			class={cn(
				'rounded-full! border-0 px-3 text-label font-semibold text-muted-foreground shadow-none data-[state=on]:bg-card! data-[state=on]:text-foreground data-[state=on]:shadow-sm',
				full && 'min-h-touch flex-1'
			)}
		>
			Sandbox
		</ToggleGroup.Item>
		<ToggleGroup.Item
			value="live"
			class={cn(
				'rounded-full! border-0 px-3 text-label font-semibold text-muted-foreground shadow-none data-[state=on]:bg-card! data-[state=on]:text-foreground data-[state=on]:shadow-sm',
				full && 'min-h-touch flex-1'
			)}
		>
			Live
		</ToggleGroup.Item>
	</ToggleGroup.Root>
{/snippet}

{#if variant === 'segmented'}
	{@render segmented('mode-toggle', true)}
{:else}
	<div class="hidden md:inline-flex">{@render segmented('mode-toggle', false)}</div>
	<button
		type="button"
		class={cn(
			'inline-flex h-control min-h-touch items-center gap-1.5 rounded-full px-3.5 text-label md:hidden',
			chipTone
		)}
		aria-label={`Data mode: ${prefs.mode === 'live' ? 'Live' : 'Sandbox'}. Change mode`}
		data-testid="mode-chip"
		onclick={openChooser}
	>
		{prefs.mode === 'live' ? 'Live' : 'Sandbox'}
		<ChevronDown class="size-4" />
	</button>
{/if}

<ResponsiveDialog
	bind:open={confirmOpen}
	title="Switch to Live mode?"
	description="You are about to view and create real payments"
	testid="dialog-live-mode"
>
	<Banner tone="warn" title="Live mode: payments move real money">
		Playground payments and payment pages created here charge real customers.
	</Banner>
	<dl class="flex flex-col divide-y rounded-md border bg-background text-body">
		<div class="flex items-center justify-between gap-3 px-3.5 py-3">
			<dt class="text-muted-foreground">Provider accounts</dt>
			<dd class="font-semibold">Live accounts only</dd>
		</div>
		<div class="flex items-center justify-between gap-3 px-3.5 py-3">
			<dt class="text-muted-foreground">Keys used</dt>
			<dd class="text-mono-id font-mono">prk_live_…</dd>
		</div>
		<div class="flex items-center justify-between gap-3 px-3.5 py-3">
			<dt class="text-muted-foreground">Switch back</dt>
			<dd>Any time from the top bar</dd>
		</div>
	</dl>
	{#snippet footer()}
		<Button variant="outline" class="flex-1 sm:flex-none" onclick={() => (confirmOpen = false)}>
			Stay in Sandbox
		</Button>
		<Button class="flex-1 sm:flex-none" onclick={goLive} data-testid="mode-confirm-live">
			Switch to Live
		</Button>
	{/snippet}
</ResponsiveDialog>

<ResponsiveDialog
	bind:open={chooserOpen}
	title="Switch to Live?"
	description="Live shows real payments and cash-ins."
	testid="sheet-mode"
>
	<div class="flex flex-col gap-2" role="radiogroup" aria-label="Data mode">
		{#each options as option (option.value)}
			<button
				type="button"
				role="radio"
				aria-checked={picked === option.value}
				class={cn(
					'flex items-start gap-3 rounded-lg border p-4 text-left',
					picked === option.value
						? 'border-primary bg-card ring-1 ring-primary'
						: 'border-border bg-card'
				)}
				onclick={() => (picked = option.value)}
			>
				<span
					class={cn(
						'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full border',
						picked === option.value
							? 'border-primary bg-primary text-primary-foreground'
							: 'border-input'
					)}
					aria-hidden="true"
				>
					{#if picked === option.value}<Check class="size-3" />{/if}
				</span>
				<span class="flex flex-col gap-0.5">
					<span class="text-card-title">{option.title}</span>
					<span class="text-helper text-muted-foreground">{option.body}</span>
				</span>
			</button>
		{/each}
	</div>
	{#if picked === 'live'}
		<Banner tone="warn" title="Live reaches real customers">
			Payments and settlements you see here are real money.
		</Banner>
	{/if}
	{#snippet footer()}
		<Button class="w-full" onclick={applyPicked} disabled={picked === prefs.mode}>
			{picked === 'live' ? 'Switch to Live' : 'Switch to Sandbox'}
		</Button>
		<Button variant="outline" class="w-full" onclick={() => (chooserOpen = false)}>
			{prefs.mode === 'live' ? 'Stay in Live' : 'Stay in Sandbox'}
		</Button>
	{/snippet}
</ResponsiveDialog>
