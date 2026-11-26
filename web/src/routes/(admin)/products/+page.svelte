<script lang="ts">
	import { resolve } from '$app/paths';
	import { toast } from 'svelte-sonner';
	import { adminApi } from '#lib/api/admin';
	import { errorMessage } from '#lib/api/client';
	import type { ApiKey, ApiKeyCreated, Meta, Product, ProviderAccount } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { loadMeta, providerLabels } from '#lib/state/catalog.svelte';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';
	import ConfirmDialog from '#lib/components/app/confirm-dialog.svelte';
	import OneTimeSecret from '#lib/components/app/one-time-secret.svelte';
	import ProductCard from '#lib/components/app/product-card.svelte';
	import ProductForm from '#lib/components/app/product-form.svelte';
	import { blankProductForm, type ProductFormValues } from '#lib/product-form';
	import OnboardingChecklist from '#lib/components/app/onboarding-checklist.svelte';
	import type { ChecklistStep } from '#lib/checklist';
	import RadioCards from '#lib/components/app/radio-cards.svelte';
	import DangerZone from '#lib/components/app/danger-zone.svelte';
	import DetailBox from '#lib/components/app/detail-box.svelte';
	import FormField from '#lib/components/app/form-field.svelte';
	import Plus from '@lucide/svelte/icons/plus';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';
	import Archive from '@lucide/svelte/icons/archive';

	type Reveal = {
		title: string;
		description: string;
		successTitle?: string;
		bannerTitle?: string;
		bannerBody: string;
		label: string;
		value: string;
		helper: string;
		signature?: boolean;
		doneLabel: string;
	};

	const products = new Resource<{ data: Product[] }>();
	const providers = new Resource<{ data: ProviderAccount[] }>();
	const meta = new Resource<Meta>();
	const list = $derived(products.data?.data ?? []);
	const labels = $derived(providerLabels(meta.data));
	const providerOptions = $derived([
		{ value: '', label: 'No default' },
		...Object.entries(labels).map(([value, label]) => ({ value, label }))
	]);
	const phone = new IsMobile(768);

	$effect(() => {
		void meta.load(loadMeta);
		void providers.load(() => adminApi.listProviders());
		refresh();
	});

	function refresh() {
		void products.load(() => adminApi.listProducts());
	}

	// Keys are open on desktop for the first product; phones start collapsed.
	let expanded = $state<Record<string, boolean>>({});
	let expandedInitialised = false;
	$effect(() => {
		if (expandedInitialised || list.length === 0) return;
		expandedInitialised = true;
		if (!phone.current) expanded = { [list[0].id]: true };
	});

	function toggleKeys(product: Product) {
		expanded = { ...expanded, [product.id]: !expanded[product.id] };
	}

	// One-time values (webhook secret, API key). Opened after the action that produced them.
	let revealOpen = $state(false);
	let reveal = $state<Reveal>({
		title: '',
		description: '',
		bannerBody: '',
		label: '',
		value: '',
		helper: '',
		doneLabel: 'Done'
	});

	function showReveal(next: Reveal) {
		reveal = next;
		revealOpen = true;
	}

	function secretReveal(
		title: string,
		description: string,
		successTitle: string,
		value: string
	): Reveal {
		return {
			title,
			description,
			successTitle,
			bannerBody:
				'Copy it now and store it on your server. You can rotate it later to get a new one.',
			label: 'Webhook secret',
			value,
			helper: 'Copy puts the full secret on your clipboard.',
			signature: true,
			doneLabel: 'I have copied it'
		};
	}

	// New product
	let createOpen = $state(false);
	let createForm = $state<ProductFormValues>(blankProductForm());
	let createError = $state<string | null>(null);
	let creating = $state(false);

	function openCreate() {
		createForm = blankProductForm();
		createError = null;
		createOpen = true;
	}

	async function submitCreate(values: ProductFormValues) {
		creating = true;
		createError = null;
		try {
			const created = await adminApi.createProduct({
				name: values.name.trim(),
				webhook_url: values.webhookUrl.trim() || undefined,
				default_provider: values.defaultProvider || undefined
			});
			createOpen = false;
			refresh();
			showReveal(
				secretReveal(
					'Product created',
					`${created.name} is ready. Save its webhook secret now.`,
					`Webhook secret for ${created.name}`,
					created.webhook_secret
				)
			);
		} catch (error) {
			createError = errorMessage(error);
		} finally {
			creating = false;
		}
	}

	// Edit product
	let editOpen = $state(false);
	let editTarget = $state<Product | null>(null);
	let editForm = $state<ProductFormValues>(blankProductForm());
	let editError = $state<string | null>(null);
	let saving = $state(false);

	function openEdit(product: Product) {
		editTarget = product;
		editForm = {
			name: product.name,
			webhookUrl: product.webhook_url ?? '',
			defaultProvider: product.default_provider ?? ''
		};
		editError = null;
		editOpen = true;
	}

	async function submitEdit(values: ProductFormValues) {
		if (!editTarget) return;
		saving = true;
		editError = null;
		try {
			await adminApi.updateProduct(editTarget.id, {
				name: values.name.trim(),
				webhook_url: values.webhookUrl.trim() || null,
				default_provider: values.defaultProvider || null
			});
			editOpen = false;
			toast.success('Product saved');
			refresh();
		} catch (error) {
			editError = errorMessage(error);
		} finally {
			saving = false;
		}
	}

	// Rotate webhook secret (confirm, then reveal)
	let rotateOpen = $state(false);
	let rotateTarget = $state<Product | null>(null);

	function openRotate(product: Product) {
		editOpen = false;
		rotateTarget = product;
		rotateOpen = true;
	}

	async function confirmRotate() {
		if (!rotateTarget) return;
		const result = await adminApi.rotateWebhookSecret(rotateTarget.id);
		showReveal(
			secretReveal(
				'Webhook secret rotated',
				`${rotateTarget.name} has a new webhook secret.`,
				`New webhook secret for ${rotateTarget.name}`,
				result.webhook_secret
			)
		);
	}

	// Archive product
	let archiveOpen = $state(false);
	let archiveTarget = $state<Product | null>(null);

	function openArchive(product: Product) {
		editOpen = false;
		archiveTarget = product;
		archiveOpen = true;
	}

	const archiveConsequences = $derived.by(() => {
		const product = archiveTarget;
		if (!product) return [];
		const active = product.api_keys.filter((k) => !k.revoked_at).length;
		return [
			`Hides ${product.name} from the Products list.`,
			active > 0
				? `Revokes all of its API keys. The ${active} active ${active === 1 ? 'key stops' : 'keys stop'} working immediately.`
				: 'Revokes all of its API keys.',
			'Keeps existing payments and cash-ins in the records.'
		];
	});

	async function confirmArchive() {
		if (!archiveTarget) return;
		await adminApi.archiveProduct(archiveTarget.id);
		toast.success(`Archived ${archiveTarget.name}`);
		refresh();
	}

	// API keys: create
	let keyOpen = $state(false);
	let keyTarget = $state<Product | null>(null);
	let keyMode = $state<'sandbox' | 'live'>('sandbox');
	let keyName = $state('');
	let keySubmitted = $state(false);
	let keyError = $state<string | null>(null);
	let creatingKey = $state(false);
	const keyNameError = $derived(
		keySubmitted && !keyName.trim() ? 'Enter a name for the key' : null
	);

	const modeOptions: { value: 'sandbox' | 'live'; label: string; description: string }[] = [
		{
			value: 'sandbox',
			label: 'Sandbox',
			description: 'For testing. Uses sandbox accounts and the simulator.'
		},
		{ value: 'live', label: 'Live', description: 'Real money. Uses your live provider accounts.' }
	];

	function openCreateKey(product: Product) {
		keyTarget = product;
		keyMode = 'sandbox';
		keyName = '';
		keySubmitted = false;
		keyError = null;
		keyOpen = true;
	}

	async function submitKey(event: SubmitEvent) {
		event.preventDefault();
		keySubmitted = true;
		if (!keyTarget || !keyName.trim()) return;
		const product = keyTarget;
		creatingKey = true;
		keyError = null;
		try {
			const created: ApiKeyCreated = await adminApi.createApiKey(product.id, {
				mode: keyMode,
				name: keyName.trim()
			});
			keyOpen = false;
			refresh();
			showReveal({
				title: 'API key created',
				description: `${product.name} · ${created.name} · ${created.mode === 'live' ? 'Live' : 'Sandbox'}`,
				bannerTitle: 'This key is shown once',
				bannerBody:
					"Copy it into your product backend now. You can't see it again. Create a new key if you lose this one.",
				label: 'API key',
				value: created.key,
				helper:
					created.mode === 'live'
						? 'Copy puts the full key on your clipboard. Live keys start with prk_live_.'
						: 'Copy puts the full key on your clipboard. Sandbox keys start with prk_test_.',
				doneLabel: 'Done'
			});
		} catch (error) {
			keyError = errorMessage(error);
		} finally {
			creatingKey = false;
		}
	}

	// API keys: revoke
	let revokeOpen = $state(false);
	let revokeTarget = $state<{ product: Product; key: ApiKey } | null>(null);

	function openRevoke(product: Product, key: ApiKey) {
		revokeTarget = { product, key };
		revokeOpen = true;
	}

	async function confirmRevoke() {
		if (!revokeTarget) return;
		await adminApi.revokeApiKey(revokeTarget.product.id, revokeTarget.key.id);
		toast.success('API key revoked');
		refresh();
	}

	// Onboarding (no products yet)
	const providerCount = $derived(providers.data?.data.length ?? 0);
	const checklist = $derived<ChecklistStep[]>([
		{
			title: providerCount > 0 ? 'Provider account connected' : 'Add a provider account',
			helper:
				providerCount > 0
					? `${providerCount} provider ${providerCount === 1 ? 'account is' : 'accounts are'} set up. Server keys are saved and masked.`
					: 'Connect a sandbox provider account first so the test flow can run.',
			status: providerCount > 0 ? 'done' : 'next',
			actionLabel: providerCount > 0 ? 'View account' : 'Add account',
			href: resolve('/(admin)/providers')
		},
		{
			title: 'Create a product',
			helper: 'Products group payments, API keys and the webhook URL. Start with your first app.',
			status: providerCount > 0 ? 'next' : 'todo',
			actionLabel: 'Create product',
			onAction: openCreate
		},
		{
			title: 'Create an API key',
			helper: 'Keys belong to a product, so create one after step 2.',
			status: 'todo',
			actionLabel: 'Create API key'
		}
	]);
</script>

<div data-testid="page-products" class="flex flex-col gap-section">
	<PageHeader
		title="Products"
		description="Each product gets its own API keys and webhook secret. Archived products are hidden."
	>
		{#snippet actions()}
			<Button onclick={openCreate} data-testid="new-product-button">
				<Plus data-icon="inline-start" />
				New product
			</Button>
		{/snippet}
	</PageHeader>

	<ErrorAlert error={products.error} subject="products" onretry={refresh} />

	{#if (products.loading && !products.data) || (providers.loading && !providers.data) || (meta.loading && !meta.data)}
		<div class="flex flex-col gap-section" aria-busy="true" aria-label="Loading products">
			{#each Array.from({ length: 3 }, (_, i) => i) as i (i)}
				<Skeleton class="h-28 w-full rounded-lg" />
			{/each}
		</div>
	{:else if list.length === 0}
		<OnboardingChecklist
			title="Get your first payment running"
			description="Three steps, all in sandbox. Nothing here charges a real customer."
			steps={checklist}
		/>
	{:else}
		<div class="flex flex-col gap-section" data-testid="product-list">
			{#each list as product (product.id)}
				<ProductCard
					{product}
					providerLabel={product.default_provider
						? (labels[product.default_provider] ?? product.default_provider)
						: 'No default'}
					expanded={!!expanded[product.id]}
					onToggle={() => toggleKeys(product)}
					onEdit={() => openEdit(product)}
					onRotate={() => openRotate(product)}
					onArchive={() => openArchive(product)}
					onCreateKey={() => openCreateKey(product)}
					onRevoke={(key: ApiKey) => openRevoke(product, key)}
				/>
			{/each}
		</div>
	{/if}
</div>

<!-- New product -->
<ResponsiveDialog
	bind:open={createOpen}
	testid="dialog-new-product"
	title="New product"
	description="Products own API keys and a webhook secret."
>
	{#if createError}
		<Banner tone="error">{createError}</Banner>
	{/if}
	<ProductForm
		id="product-create"
		bind:values={createForm}
		{providerOptions}
		onvalid={submitCreate}
	/>
	{#snippet footer()}
		<Button
			type="button"
			variant="outline"
			class="w-full sm:w-auto"
			onclick={() => (createOpen = false)}
		>
			Cancel
		</Button>
		<Button type="submit" form="product-create" class="w-full sm:w-auto" disabled={creating}>
			{creating ? 'Creating…' : 'Create product'}
		</Button>
	{/snippet}
</ResponsiveDialog>

<!-- Edit product -->
<ResponsiveDialog
	bind:open={editOpen}
	testid="dialog-edit-product"
	title={`Edit ${editTarget?.name ?? 'product'}`}
	description="Clear the webhook URL to stop deliveries."
>
	{#if editTarget}
		{#if editError}
			<Banner tone="error">{editError}</Banner>
		{/if}
		<ProductForm id="product-edit" bind:values={editForm} {providerOptions} onvalid={submitEdit} />

		<div
			class="flex flex-col gap-2 rounded-md border p-3.5 sm:flex-row sm:items-center sm:justify-between"
		>
			<div class="flex min-w-0 flex-col gap-1">
				<span class="text-label text-foreground">Webhook secret</span>
				<span class="text-helper text-muted-foreground">
					Signs every webhook. Rotating makes the current secret stop verifying at once.
				</span>
			</div>
			<Button variant="outline" class="shrink-0" onclick={() => openRotate(editTarget!)}>
				<RotateCw data-icon="inline-start" />
				Rotate secret
			</Button>
		</div>

		<DangerZone
			title="Danger zone"
			description={`Archiving hides ${editTarget.name} and revokes its ${editTarget.api_keys.filter((k) => !k.revoked_at).length} active keys. Payment records stay in the dashboard.`}
			actionLabel="Archive product"
			icon={Archive}
			onaction={() => openArchive(editTarget!)}
		/>
	{/if}
	{#snippet footer()}
		<Button
			type="button"
			variant="outline"
			class="w-full sm:w-auto"
			onclick={() => (editOpen = false)}
		>
			Cancel
		</Button>
		<Button type="submit" form="product-edit" class="w-full sm:w-auto" disabled={saving}>
			{saving ? 'Saving…' : 'Save'}
		</Button>
	{/snippet}
</ResponsiveDialog>

<!-- Rotate webhook secret -->
<ConfirmDialog
	bind:open={rotateOpen}
	testid="dialog-rotate-secret"
	title="Rotate webhook secret?"
	description={rotateTarget?.name ?? ''}
	consequences={['The new secret is shown once, on the next screen.']}
	confirmLabel="Rotate secret"
	destructive
	onconfirm={confirmRotate}
>
	<Banner tone="warn" title="Update your server first">
		The current secret stops verifying signatures at once. Put the new secret in your server before
		you rotate, or deliveries fail verification.
	</Banner>
</ConfirmDialog>

<!-- Archive product -->
<ConfirmDialog
	bind:open={archiveOpen}
	testid="dialog-archive-product"
	title={`Archive ${archiveTarget?.name ?? 'product'}?`}
	description="Archived products leave the Products list."
	consequences={archiveConsequences}
	confirmLabel="Archive product"
	destructive
	onconfirm={confirmArchive}
/>

<!-- New API key -->
<ResponsiveDialog
	bind:open={keyOpen}
	testid="dialog-create-key"
	title="New API key"
	description={`For ${keyTarget?.name ?? 'this product'}. A sandbox key reaches only sandbox provider accounts.`}
>
	<form id="create-key-form" class="flex flex-col gap-5" novalidate onsubmit={submitKey}>
		{#if keyError}
			<Banner tone="error">{keyError}</Banner>
		{/if}
		<RadioCards
			name="key-mode"
			legend="Mode"
			testid="key-mode"
			options={modeOptions}
			bind:value={keyMode}
		/>
		<FormField
			id="key-name"
			label="Key name"
			error={keyNameError}
			helper="Shown in the key list, for example backend or checkout-worker."
		>
			<Input
				id="key-name"
				placeholder="backend"
				maxlength={80}
				autocomplete="off"
				aria-invalid={!!keyNameError || undefined}
				bind:value={keyName}
			/>
		</FormField>
		<Banner tone="info" title="Keys start with prk_test_ or prk_live_">
			Sandbox keys start with prk_test_ and Live keys with prk_live_. The full key is shown once
			after you create it.
		</Banner>
	</form>
	{#snippet footer()}
		<Button
			type="button"
			variant="outline"
			class="w-full sm:w-auto"
			onclick={() => (keyOpen = false)}
		>
			Cancel
		</Button>
		<Button type="submit" form="create-key-form" class="w-full sm:w-auto" disabled={creatingKey}>
			<Plus data-icon="inline-start" />
			{creatingKey ? 'Creating…' : 'Create key'}
		</Button>
	{/snippet}
</ResponsiveDialog>

<!-- Revoke API key -->
<ConfirmDialog
	bind:open={revokeOpen}
	testid="dialog-revoke-key"
	title="Revoke API key?"
	description={revokeTarget
		? `${revokeTarget.product.name} · ${revokeTarget.key.mode === 'live' ? 'Live' : 'Sandbox'}`
		: ''}
	confirmLabel="Revoke key"
	destructive
	onconfirm={confirmRevoke}
>
	{#if revokeTarget}
		<DetailBox
			rows={[
				{ label: 'Key name', value: revokeTarget.key.name },
				{ label: 'Mode', value: revokeTarget.key.mode === 'live' ? 'Live' : 'Sandbox' },
				{ label: 'Prefix', value: `${revokeTarget.key.prefix}…`, mono: true }
			]}
		/>
		<p class="text-body text-foreground">
			"{revokeTarget.key.name}" stops working immediately. Any app that still uses this key starts
			getting errors on its next request. This cannot be undone.
		</p>
	{/if}
</ConfirmDialog>

<!-- Shown once: webhook secret or API key -->
<OneTimeSecret bind:open={revealOpen} testid="dialog-secret-reveal" {...reveal} />
