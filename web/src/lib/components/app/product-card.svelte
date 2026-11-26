<script lang="ts">
	/**
	 * One product: identity, webhook URL, default provider and actions. The API keys are a
	 * disclosure inside the card (the table is `ApiKeyTable`), so keys stay with their product.
	 * Below lg the identity stacks above the actions, so the webhook URL keeps the card width.
	 */
	import type { ApiKey, Product } from '#lib/api/types';
	import ApiKeyTable from '#lib/components/app/api-key-table.svelte';
	import OverflowMenu, { type MenuItem } from '#lib/components/app/overflow-menu.svelte';
	import { Button } from '#lib/components/ui/button';
	import Archive from '@lucide/svelte/icons/archive';
	import Building from '@lucide/svelte/icons/landmark';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Pencil from '@lucide/svelte/icons/pencil';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';
	import Pill from '#lib/components/app/pill.svelte';
	import { cn } from '#lib/utils';
	import { keyDate } from '#lib/key-format';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';

	let {
		product,
		providerLabel,
		expanded,
		onToggle,
		onEdit,
		onRotate,
		onArchive,
		onCreateKey,
		onRevoke
	}: {
		product: Product;
		/** Human label of the default provider, or "No default". */
		providerLabel: string;
		expanded: boolean;
		onToggle: () => void;
		onEdit: () => void;
		onRotate: () => void;
		onArchive: () => void;
		onCreateKey: () => void;
		onRevoke: (key: ApiKey) => void;
	} = $props();

	// At 320 the keys are always listed (design); from 360 up they sit behind the disclosure.
	const tiny = new IsMobile(360);
	const open = $derived(expanded || tiny.current);
	const initials = $derived(
		product.name
			.split(/\s+/)
			.map((w) => w[0])
			.join('')
			.slice(0, 2)
			.toUpperCase()
	);
	const activeKeys = $derived(product.api_keys.filter((k) => !k.revoked_at).length);
	const keysId = $derived(`product-${product.id}-keys`);

	const menuItems = $derived<MenuItem[]>([
		{ label: 'Edit', icon: Pencil, onselect: onEdit },
		{ label: 'Rotate secret', icon: RotateCw, onselect: onRotate },
		{ label: 'Archive product', icon: Archive, destructive: true, onselect: onArchive }
	]);
</script>

<!-- Below lg the card is one column: identity, keys box, then Edit / API keys (design order). -->
<article
	class="rounded-lg border bg-card max-lg:flex max-lg:flex-col max-lg:gap-4 max-lg:p-card"
	data-testid="product-row"
	data-product-slug={product.slug}
>
	<div class="flex flex-col gap-4 p-card max-lg:contents lg:flex-row lg:items-center lg:gap-6">
		<div
			class="flex min-w-0 flex-1 flex-col gap-3 max-lg:order-1 lg:flex-row lg:items-center lg:gap-6"
		>
			<div class="flex min-w-0 items-center gap-3 lg:w-56 lg:shrink-0">
				<span
					class="flex size-11 shrink-0 items-center justify-center rounded-md bg-secondary text-label text-foreground lg:hidden"
					aria-hidden="true"
				>
					{initials}
				</span>
				<div class="flex min-w-0 flex-col gap-0.5">
					<h2 class="truncate text-card-title text-foreground">{product.name}</h2>
					<span class="truncate font-mono text-helper text-muted-foreground">{product.slug}</span>
				</div>
			</div>
			<div class="flex items-center justify-between gap-3 text-body lg:hidden">
				<span class="text-muted-foreground">Default provider</span>
				<span class="font-semibold">{providerLabel}</span>
			</div>
			<div class="flex min-w-0 flex-1 flex-col gap-1">
				<span class="text-helper text-muted-foreground">Webhook URL</span>
				{#if product.webhook_url}
					<span class="text-mono-id font-mono break-all text-foreground">
						{product.webhook_url}
					</span>
				{:else}
					<span class="text-mono-id font-mono text-muted-foreground">
						Not set. Deliveries are skipped.
					</span>
				{/if}
			</div>
			<div class="flex items-center justify-between gap-3 text-body lg:hidden">
				<span class="text-muted-foreground">Created</span>
				<span class="tabular font-semibold">{keyDate(product.created_at)}</span>
			</div>
			<div class="hidden lg:block lg:shrink-0">
				<Pill tone="neutral">
					<Building class="size-3.5" />
					{providerLabel}
				</Pill>
			</div>
		</div>
		<div
			class="flex flex-wrap items-center gap-2 max-lg:order-3 max-lg:grid max-lg:grid-cols-[1fr_1fr_auto] max-xs:grid-cols-[1fr_auto] lg:shrink-0 lg:flex-nowrap"
		>
			<Button variant="outline" onclick={onEdit} aria-label="Edit {product.name}">
				<Pencil data-icon="inline-start" />
				Edit
			</Button>
			<Button
				variant="outline"
				class="max-xs:hidden lg:hidden"
				onclick={onToggle}
				aria-label="Toggle API keys of {product.name}"
				aria-expanded={open}
			>
				<KeyRound data-icon="inline-start" />
				API keys
			</Button>
			<Button variant="outline" class="hidden lg:inline-flex" onclick={onRotate}>
				<RotateCw data-icon="inline-start" />
				Rotate secret
			</Button>
			<OverflowMenu
				label="More actions for {product.name}"
				items={menuItems}
				testid="product-menu"
			/>
		</div>
	</div>

	<div class="border-t max-lg:order-2 max-lg:rounded-md max-lg:border-t-0 max-lg:bg-secondary">
		<button
			type="button"
			class="flex min-h-touch w-full items-center justify-between gap-3 px-card py-3 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 max-lg:px-3"
			aria-expanded={open}
			aria-controls={keysId}
			onclick={onToggle}
		>
			<span class="text-label text-foreground">API keys</span>
			<span class="flex items-center gap-2 text-helper text-muted-foreground">
				{activeKeys} of {product.api_keys.length} active
				<ChevronDown class={cn('size-4 transition-transform', open && 'rotate-180')} />
			</span>
		</button>
		{#if open}
			<div id={keysId} class="px-card pb-card max-lg:bg-card max-lg:px-3 max-lg:pb-3">
				<ApiKeyTable keys={product.api_keys} onCreate={onCreateKey} {onRevoke} />
			</div>
		{/if}
	</div>
</article>
