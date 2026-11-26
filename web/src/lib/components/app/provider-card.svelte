<script lang="ts">
	/**
	 * One provider account: enabled switch, masked credentials, the inbound webhook URL to paste
	 * into the provider, the fee summary and the Edit / Test actions. A test result shows inline.
	 */
	import type { ProviderAccount, ProviderTestResult } from '#lib/api/types';
	import { PASTE_HINT, PROVIDER_CODE, feeSummary } from '#lib/provider-display';
	import CopyButton from '#lib/components/app/copy-button.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import TestResult from '#lib/components/app/test-result.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Switch } from '#lib/components/ui/switch';
	import PlugZap from '@lucide/svelte/icons/plug-zap';
	import Pencil from '@lucide/svelte/icons/pencil';

	let {
		account,
		providerLabel,
		credentialLabel,
		methodLabels,
		testResult,
		testing,
		toggling,
		onToggle,
		onEdit,
		onTest
	}: {
		account: ProviderAccount;
		providerLabel: string;
		credentialLabel: (key: string) => string;
		methodLabels: Record<string, string>;
		testResult: ProviderTestResult | null;
		testing: boolean;
		toggling: boolean;
		onToggle: (enabled: boolean) => void;
		onEdit: () => void;
		onTest: () => void;
	} = $props();

	const isMock = $derived(account.provider === 'mock');
	const isLive = $derived(account.mode === 'live');
	const credentials = $derived(Object.entries(account.credentials));
</script>

<article
	class="flex flex-col gap-4 rounded-lg border bg-card p-card"
	data-testid="provider-row"
	data-provider={account.provider}
	data-mode={account.mode}
>
	<header class="flex items-start gap-3">
		<div
			class="flex size-11 shrink-0 items-center justify-center rounded-md bg-secondary text-label text-foreground"
			aria-hidden="true"
		>
			{PROVIDER_CODE[account.provider] ?? account.provider.slice(0, 2).toUpperCase()}
		</div>
		<div class="flex min-w-0 flex-1 flex-col gap-1.5">
			<h2 class="truncate text-card-title text-foreground">{account.label}</h2>
			<div class="flex flex-wrap items-center gap-2">
				<Pill tone={isLive ? 'success' : 'sandbox'}>{isLive ? 'Live' : 'Sandbox'}</Pill>
				<span class="text-helper text-muted-foreground">{providerLabel}</span>
			</div>
		</div>
		<label class="flex min-h-touch shrink-0 cursor-pointer items-center gap-2">
			<span class="text-helper text-muted-foreground">
				{account.enabled ? 'Enabled' : 'Disabled'}
			</span>
			<Switch
				checked={account.enabled}
				disabled={toggling}
				aria-label={`Enable ${account.label}`}
				onCheckedChange={(checked: boolean) => onToggle(checked)}
			/>
		</label>
	</header>

	{#if credentials.length > 0}
		<dl class="flex flex-col gap-2 text-body">
			{#each credentials as [key, value] (key)}
				<div class="flex items-center justify-between gap-4">
					<dt class="text-muted-foreground">{credentialLabel(key)}</dt>
					<dd class="tabular min-w-0 truncate text-mono-id font-mono text-foreground">
						{value || 'Not set'}
					</dd>
				</div>
			{/each}
		</dl>
	{/if}

	<div class="flex flex-col gap-2">
		<span class="text-label text-foreground">Inbound webhook URL</span>
		{#if isMock}
			<div class="rounded-md bg-secondary px-3 py-2.5 text-mono-id font-mono text-muted-foreground">
				No webhook: use the simulator on the Snap page
			</div>
			<p class="text-helper text-muted-foreground">
				Runs through the same status pipeline as a real provider, so you can test paid, expired and
				failed flows.
			</p>
		{:else}
			<div class="flex items-center gap-2 rounded-md border bg-background py-1.5 pr-1.5 pl-3">
				<code class="min-w-0 flex-1 truncate text-mono-id font-mono text-foreground">
					{account.webhook_url}
				</code>
				<CopyButton text={account.webhook_url} label="Copy" />
			</div>
			<p class="text-helper text-muted-foreground">{PASTE_HINT[account.provider] ?? ''}</p>
		{/if}
	</div>

	<footer class="flex flex-col gap-3 border-t pt-4 md:flex-row md:items-end md:justify-between">
		<div class="flex min-w-0 flex-col gap-0.5">
			<span class="text-helper text-muted-foreground">Fee schedule</span>
			<span class="text-body text-foreground">
				{feeSummary(account.provider, account.fees, methodLabels)}
			</span>
		</div>
		<div class="grid grid-cols-2 gap-2 md:flex md:shrink-0">
			<Button variant="outline" onclick={onEdit} aria-label="Edit {account.label}">
				<Pencil data-icon="inline-start" />
				Edit
			</Button>
			<Button onclick={onTest} disabled={testing} aria-label="Test connection for {account.label}">
				<PlugZap data-icon="inline-start" />
				{testing ? 'Testing…' : 'Test connection'}
			</Button>
		</div>
	</footer>

	<TestResult result={testResult} />
</article>
