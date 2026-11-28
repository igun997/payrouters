<script lang="ts">
	/**
	 * API keys of one product. Rows are a grid once the keys box is wide enough (container query,
	 * not the viewport: the sidebar and card padding eat into it) and stacked cards below, so one
	 * DOM serves every width. Full key values are never listed: only the prefix.
	 */
	import type { ApiKey } from '#lib/api/types';
	import { keyDate, keyLastUsed } from '#lib/key-format';
	import Pill from '#lib/components/app/pill.svelte';
	import { Button } from '#lib/components/ui/button';
	import Plus from '@lucide/svelte/icons/plus';
	import Trash2 from '@lucide/svelte/icons/trash-2';

	let {
		keys,
		onCreate,
		onRevoke
	}: {
		keys: ApiKey[];
		onCreate: () => void;
		onRevoke: (key: ApiKey) => void;
	} = $props();

	const activeCount = $derived(keys.filter((k) => !k.revoked_at).length);
</script>

<div class="@container flex flex-col gap-3">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<p class="min-w-0 flex-1 text-helper text-muted-foreground max-sm:basis-full">
			A sandbox key reaches only sandbox provider accounts; a live key reaches live ones. Full keys
			are shown once.
		</p>
		<Button size="sm" onclick={onCreate} data-testid="create-api-key-button">
			<Plus data-icon="inline-start" />
			API key
		</Button>
	</div>

	{#if keys.length === 0}
		<p
			class="rounded-md border border-dashed px-4 py-6 text-center text-body text-muted-foreground"
			data-testid="api-key-empty"
		>
			No keys yet. Create one to connect your backend.
		</p>
	{:else}
		<ul
			class="flex flex-col overflow-hidden rounded-md border"
			data-testid="api-key-list"
			aria-label="API keys"
			data-active={activeCount}
		>
			<li
				aria-hidden="true"
				class="hidden gap-x-4 bg-secondary px-4 py-2.5 text-helper font-semibold text-muted-foreground @4xl:grid @4xl:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.2fr)_auto_auto_auto_auto]"
			>
				<span>Name</span>
				<span>Mode</span>
				<span>Prefix</span>
				<span>Created</span>
				<span>Last used</span>
				<span>Status</span>
				<span></span>
			</li>
			{#each keys as key (key.id)}
				{@const revoked = key.revoked_at !== null}
				<li
					class="flex flex-col gap-2 border-t px-4 py-3 @4xl:grid @4xl:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.2fr)_auto_auto_auto_auto] @4xl:items-center @4xl:gap-x-4 @4xl:py-2.5"
					data-testid="api-key-row"
					data-status={revoked ? 'revoked' : 'active'}
				>
					<div class="flex min-w-0 items-center justify-between gap-2 @4xl:contents">
						<span class="min-w-0 text-cell font-medium break-words text-foreground">
							{key.name}
						</span>
						<Pill tone={key.mode === 'live' ? 'success' : 'sandbox'}>
							{key.mode === 'live' ? 'Live' : 'Sandbox'}
						</Pill>
					</div>
					<span class="min-w-0 text-mono-id font-mono break-all text-muted-foreground">
						{key.prefix}…
					</span>
					<div
						class="tabular flex flex-wrap gap-x-4 gap-y-1 text-helper text-muted-foreground @4xl:contents"
					>
						<span class="@4xl:text-cell">Created {keyDate(key.created_at)}</span>
						<span class="@4xl:text-cell">
							Last used {keyLastUsed(key.last_used_at)}
						</span>
					</div>
					<div class="flex items-center justify-between gap-2 @4xl:contents">
						<span>
							<Pill tone={revoked ? 'neutral' : 'success'}>{revoked ? 'Revoked' : 'Active'}</Pill>
						</span>
						{#if revoked}
							<span></span>
						{:else}
							<Button
								variant="ghost"
								size="sm"
								class="justify-self-end text-destructive hover:text-destructive"
								aria-label="Revoke {key.name}"
								onclick={() => onRevoke(key)}
							>
								<Trash2 data-icon="inline-start" />
								Revoke
							</Button>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
