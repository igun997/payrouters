<script lang="ts">
	import { toast } from 'svelte-sonner';
	import { adminApi } from '#lib/api/admin';
	import { ApiError, errorMessage } from '#lib/api/client';
	import type { Meta, Provider, ProviderAccount, ProviderTestResult } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import {
		loadMeta,
		methodLabels as methodLabelMap,
		providerLabels
	} from '#lib/state/catalog.svelte';
	import { rowsToFees } from '#lib/fees';
	import { formatDateTime } from '#lib/format';
	import { Button } from '#lib/components/ui/button';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';
	import ConfirmDialog from '#lib/components/app/confirm-dialog.svelte';
	import DetailBox from '#lib/components/app/detail-box.svelte';
	import EmptyState from '#lib/components/app/empty-state.svelte';
	import ProviderCard from '#lib/components/app/provider-card.svelte';
	import ProviderForm from '#lib/components/app/provider-form.svelte';
	import {
		blankProviderForm,
		credentialPayload,
		valuesFromAccount,
		type ProviderFormValues
	} from '#lib/provider-form';
	import Plus from '@lucide/svelte/icons/plus';
	import Landmark from '@lucide/svelte/icons/landmark';
	import Trash2 from '@lucide/svelte/icons/trash-2';

	const accounts = new Resource<{ data: ProviderAccount[] }>();
	const meta = new Resource<Meta>();
	const list = $derived(accounts.data?.data ?? []);
	const metaProviders = $derived(meta.data?.providers ?? []);
	const providerNames = $derived(providerLabels(meta.data));
	const methodNames = $derived(methodLabelMap(meta.data));

	$effect(() => {
		void meta.load(loadMeta);
		refresh();
	});

	function refresh() {
		void accounts.load(() => adminApi.listProviders());
	}

	/** Banner for an action that failed outside a dialog (toggle, disable). */
	let pageError = $state<string | null>(null);
	let togglingId = $state<string | null>(null);

	async function toggleEnabled(account: ProviderAccount, enabled: boolean) {
		togglingId = account.id;
		pageError = null;
		try {
			await adminApi.updateProvider(account.id, { enabled });
			refresh();
		} catch (error) {
			pageError = errorMessage(error);
		} finally {
			togglingId = null;
		}
	}

	// Test connection: the result stays under the account until the next test.
	let testingId = $state<string | null>(null);
	let testResults = $state<Record<string, ProviderTestResult>>({});

	async function testConnection(account: ProviderAccount) {
		testingId = account.id;
		try {
			const result = await adminApi.testProvider(account.id);
			testResults = { ...testResults, [account.id]: result };
		} catch (error) {
			testResults = { ...testResults, [account.id]: { ok: false, message: errorMessage(error) } };
		} finally {
			testingId = null;
		}
	}

	function credentialLabel(provider: Provider, key: string): string {
		return (
			metaProviders.find((p) => p.id === provider)?.credential_fields.find((f) => f.key === key)
				?.label ?? key
		);
	}

	// Add / edit
	let formOpen = $state(false);
	let formMode = $state<'create' | 'edit'>('create');
	let formTarget = $state<ProviderAccount | null>(null);
	let formValues = $state<ProviderFormValues>(blankProviderForm());
	let formError = $state<string | null>(null);
	let saving = $state(false);

	const formProvider = $derived(
		metaProviders.find(
			(p) => p.id === (formMode === 'edit' ? formTarget?.provider : formValues.provider)
		)
	);
	const formTitle = $derived(
		formMode === 'edit' ? `Edit ${formTarget?.label ?? 'account'}` : 'Add provider account'
	);
	const formDescription = $derived(
		formMode === 'edit'
			? 'Leave a secret as it is to keep the stored value.'
			: formProvider && !formProvider.supports_sandbox
				? `${formProvider.label} accounts are live only. Credentials are stored encrypted.`
				: 'Credentials are sent once and stored encrypted; secrets are never shown again.'
	);

	function openAdd() {
		formMode = 'create';
		formTarget = null;
		formValues = blankProviderForm();
		formError = null;
		formOpen = true;
	}

	function openEdit(account: ProviderAccount) {
		formMode = 'edit';
		formTarget = account;
		formValues = valuesFromAccount(
			account,
			metaProviders.find((p) => p.id === account.provider)
		);
		formError = null;
		formOpen = true;
	}

	async function submitForm(values: ProviderFormValues) {
		saving = true;
		formError = null;
		try {
			if (formMode === 'create') {
				const info = metaProviders.find((p) => p.id === values.provider);
				if (!info || !values.provider) return;
				await adminApi.createProvider({
					provider: values.provider,
					mode: values.mode,
					label: values.label.trim() || undefined,
					credentials: credentialPayload(values, info.credential_fields),
					fees: rowsToFees(values.fees)
				});
				toast.success('Provider account created');
			} else if (formTarget) {
				const info = metaProviders.find((p) => p.id === formTarget?.provider);
				const credentials = credentialPayload(values, info?.credential_fields ?? []);
				await adminApi.updateProvider(formTarget.id, {
					label: values.label.trim() || undefined,
					enabled: values.enabled,
					credentials: Object.keys(credentials).length > 0 ? credentials : undefined,
					fees: rowsToFees(values.fees)
				});
				toast.success('Provider account saved');
			}
			formOpen = false;
			refresh();
		} catch (error) {
			formError = errorMessage(error);
		} finally {
			saving = false;
		}
	}

	// Delete (a 409 means payments reference the account: offer to disable it instead)
	let deleteOpen = $state(false);
	let deleteTarget = $state<ProviderAccount | null>(null);

	function askDelete(account: ProviderAccount) {
		formOpen = false;
		deleteTarget = account;
		deleteOpen = true;
	}

	async function confirmDelete() {
		const target = deleteTarget;
		if (!target) return;
		try {
			await adminApi.deleteProvider(target.id);
		} catch (error) {
			if (error instanceof ApiError && error.status === 409) {
				await openInUse(target);
				return;
			}
			throw error;
		}
		toast.success('Provider account deleted');
		refresh();
	}

	let inUseOpen = $state(false);
	let inUseTarget = $state<ProviderAccount | null>(null);
	let inUseLatest = $state('—');
	let inUseError = $state<string | null>(null);
	let disabling = $state(false);

	async function openInUse(account: ProviderAccount) {
		inUseTarget = account;
		inUseLatest = '—';
		inUseError = null;
		inUseOpen = true;
		try {
			const page = await adminApi.listPayments({
				provider: account.provider,
				mode: account.mode,
				limit: 1
			});
			const latest = page.data[0];
			if (latest) inUseLatest = `${latest.order_id} · ${formatDateTime(latest.created_at)}`;
		} catch {
			// The count is context only; the dialog works without it.
		}
	}

	async function disableInUse() {
		if (!inUseTarget) return;
		disabling = true;
		inUseError = null;
		try {
			await adminApi.updateProvider(inUseTarget.id, { enabled: false });
			inUseOpen = false;
			toast.success('Account disabled');
			refresh();
		} catch (error) {
			inUseError = errorMessage(error);
		} finally {
			disabling = false;
		}
	}
</script>

<div data-testid="page-providers" class="flex flex-col gap-section">
	<PageHeader
		title="Provider accounts"
		description="One account per provider and mode. Secrets are write-only and always shown masked."
	>
		{#snippet actions()}
			<Button
				onclick={openAdd}
				disabled={metaProviders.length === 0}
				data-testid="add-provider-button"
			>
				<Plus data-icon="inline-start" />
				Add account
			</Button>
		{/snippet}
	</PageHeader>

	<ErrorAlert error={accounts.error} onretry={refresh} />
	{#if pageError}
		<Banner tone="error">{pageError}</Banner>
	{/if}

	{#if accounts.loading && !accounts.data}
		<div
			class="grid grid-cols-1 gap-section md:grid-cols-2"
			aria-busy="true"
			aria-label="Loading provider accounts"
		>
			{#each Array.from({ length: 2 }, (_, i) => i) as i (i)}
				<Skeleton class="h-72 w-full rounded-lg" />
			{/each}
		</div>
	{:else if list.length === 0}
		<EmptyState
			dashed
			icon={Landmark}
			title="No provider accounts"
			description="Add a sandbox account first to test the full flow."
		>
			{#snippet actions()}
				<Button onclick={openAdd} disabled={metaProviders.length === 0}>
					<Plus data-icon="inline-start" />
					Add account
				</Button>
			{/snippet}
		</EmptyState>
	{:else}
		<div class="grid grid-cols-1 gap-section md:grid-cols-2" data-testid="provider-list">
			{#each list as account (account.id)}
				<ProviderCard
					{account}
					providerLabel={providerNames[account.provider] ?? account.provider}
					credentialLabel={(key: string) => credentialLabel(account.provider, key)}
					methodLabels={methodNames}
					testResult={testResults[account.id] ?? null}
					testing={testingId === account.id}
					toggling={togglingId === account.id}
					onToggle={(enabled: boolean) => toggleEnabled(account, enabled)}
					onEdit={() => openEdit(account)}
					onTest={() => testConnection(account)}
				/>
			{/each}
		</div>
	{/if}
</div>

<!-- Add / edit provider account -->
<ResponsiveDialog
	bind:open={formOpen}
	testid={formMode === 'edit' ? 'dialog-edit-provider' : 'dialog-add-provider'}
	title={formTitle}
	description={formDescription}
	size="lg"
	fullScreenOnPhone
>
	{#if formError}
		<Banner tone="error">{formError}</Banner>
	{/if}
	<ProviderForm
		id={formMode === 'edit' ? 'provider-edit' : 'provider-create'}
		mode={formMode}
		account={formTarget}
		meta={metaProviders}
		bind:values={formValues}
		onvalid={submitForm}
	/>
	{#snippet footer()}
		{#if formMode === 'edit' && formTarget}
			<Button
				type="button"
				variant="destructive-outline"
				class="w-full sm:mr-auto sm:w-auto"
				onclick={() => formTarget && askDelete(formTarget)}
			>
				<Trash2 data-icon="inline-start" />
				Delete account
			</Button>
		{/if}
		<Button
			type="button"
			variant="outline"
			class="w-full sm:w-auto"
			onclick={() => (formOpen = false)}
		>
			Cancel
		</Button>
		<Button
			type="submit"
			form={formMode === 'edit' ? 'provider-edit' : 'provider-create'}
			class="w-full sm:w-auto"
			disabled={saving}
		>
			{#if saving}
				Saving…
			{:else}
				{formMode === 'edit' ? 'Save changes' : 'Save account'}
			{/if}
		</Button>
	{/snippet}
</ResponsiveDialog>

<!-- Delete provider account -->
<ConfirmDialog
	bind:open={deleteOpen}
	testid="dialog-delete-provider"
	title={`Delete ${deleteTarget?.label ?? 'account'}?`}
	description="Accounts that payments reference cannot be deleted. Disable the account instead to stop using it."
	confirmLabel="Delete account"
	destructive
	onconfirm={confirmDelete}
>
	<Banner tone="warn" title="What stops working">
		Its webhook URL rejects every event right away, and the stored keys and fee schedule are erased.
	</Banner>
	{#if deleteTarget}
		<DetailBox
			rows={[
				{ label: 'Provider', value: providerNames[deleteTarget.provider] ?? deleteTarget.provider },
				{ label: 'Mode', value: deleteTarget.mode === 'live' ? 'Live' : 'Sandbox' },
				{ label: 'Webhook URL', value: deleteTarget.webhook_url, mono: true }
			]}
		/>
	{/if}
</ConfirmDialog>

<!-- Provider still referenced by payments -->
<ResponsiveDialog
	bind:open={inUseOpen}
	testid="dialog-provider-in-use"
	title={`Can't delete ${inUseTarget?.label ?? 'account'}`}
	description="This account is still in use."
>
	{#if inUseError}
		<Banner tone="error">{inUseError}</Banner>
	{/if}
	<Banner tone="warn">Payments reference this account. Disable it instead of deleting.</Banner>
	{#if inUseTarget}
		<DetailBox
			rows={[
				{ label: 'Provider', value: providerNames[inUseTarget.provider] ?? inUseTarget.provider },
				{ label: 'Latest payment', value: inUseLatest, mono: true }
			]}
		/>
	{/if}
	{#snippet footer()}
		<Button
			type="button"
			variant="outline"
			class="w-full sm:w-auto"
			onclick={() => (inUseOpen = false)}
		>
			Cancel
		</Button>
		<Button class="w-full sm:w-auto" disabled={disabling} onclick={disableInUse}>
			{disabling ? 'Disabling…' : 'Disable account'}
		</Button>
	{/snippet}
</ResponsiveDialog>
