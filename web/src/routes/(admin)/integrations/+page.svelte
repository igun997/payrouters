<script lang="ts">
	// Connect AI assistants (MCP clients) to this PayRouters deployment. The server URL comes from
	// /admin/me; each client snippet points at it. Connected apps are the OAuth grants the dashboard
	// session approved on the consent screen.
	import { toast } from 'svelte-sonner';
	import { adminApi } from '#lib/api/admin';
	import type { Me, OAuthGrant, OAuthScopeKey } from '#lib/api/types';
	import { formatDateTime } from '#lib/format';
	import { Resource } from '#lib/state/resource.svelte';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import { Button } from '#lib/components/ui/button';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import CodeBlock from '#lib/components/app/code-block.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import EmptyState from '#lib/components/app/empty-state.svelte';
	import ConfirmDialog from '#lib/components/app/confirm-dialog.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import Plug from '@lucide/svelte/icons/plug';

	const me = new Resource<Me>();
	const grants = new Resource<{ data: OAuthGrant[] }>();
	const mcpUrl = $derived(me.data?.mcp_url ?? '');
	const connections = $derived(grants.data?.data ?? []);

	const scopeLabels: Record<OAuthScopeKey, string> = {
		read: 'Read',
		write: 'Make changes',
		live: 'Live mode'
	};

	$effect(() => {
		void me.load(() => adminApi.me());
		void grants.load(() => adminApi.listOAuthGrants());
	});

	let revokeTarget = $state<OAuthGrant | null>(null);
	let revokeOpen = $state(false);

	function askRevoke(grant: OAuthGrant) {
		revokeTarget = grant;
		revokeOpen = true;
	}

	// The access and refresh tokens stop working on the server as soon as it answers 204.
	async function revoke() {
		const target = revokeTarget;
		if (!target) return;
		await adminApi.revokeOAuthGrant(target.id);
		toast.success(`${target.client_name} is disconnected`);
		void grants.load(() => adminApi.listOAuthGrants());
	}
</script>

<svelte:head>
	<title>Integrations · PayRouters</title>
</svelte:head>

<PageHeader
	title="Integrations"
	description="Connect AI assistants to your PayRouters configuration."
	phoneDescription="Connect AI assistants to your PayRouters configuration."
/>

<div class="flex flex-col gap-section" data-testid="page-integrations">
	<section
		class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
		aria-labelledby="integrations-server-title"
		data-testid="integrations-server"
	>
		<h2 id="integrations-server-title" class="text-card-title">MCP server</h2>
		<p class="text-body text-muted-foreground">
			Assistants connect to this address. Nobody pastes a token: the client opens the consent screen
			in your browser.
		</p>
		{#if me.error}
			<ErrorAlert error={me.error} subject="the server address" />
		{:else if !me.data}
			<Skeleton class="h-16 w-full" />
		{:else}
			<CodeBlock code={mcpUrl} caption="Server URL" testid="mcp-url" />
		{/if}
	</section>

	<section
		class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
		aria-labelledby="integrations-connect-title"
		data-testid="integrations-connect"
	>
		<div class="flex flex-col gap-1">
			<h2 id="integrations-connect-title" class="text-card-title">Connect a client</h2>
			<p class="text-body text-muted-foreground">
				Run the command or paste the configuration. Your browser opens the consent screen, and the
				dashboard session you are signed in with approves it.
			</p>
		</div>

		{#if !me.data}
			{#if !me.error}
				<div class="flex flex-col gap-4" aria-busy="true">
					<Skeleton class="h-16 w-full" />
					<Skeleton class="h-16 w-full" />
				</div>
			{/if}
		{:else}
			<div class="flex flex-col gap-2" data-testid="client-claude-code">
				<h3 class="text-label text-foreground">Claude Code</h3>
				<CodeBlock
					code={`claude mcp add --transport http payrouters ${mcpUrl}`}
					caption="Terminal"
					testid="snippet-claude-code"
				/>
			</div>

			<div class="flex flex-col gap-2" data-testid="client-claude-desktop">
				<h3 class="text-label text-foreground">Claude Desktop and claude.ai</h3>
				<p class="text-body text-muted-foreground">
					Open Settings, choose Connectors, select Add custom connector and paste the server URL
					above.
				</p>
			</div>

			<div class="flex flex-col gap-2" data-testid="client-cursor">
				<h3 class="text-label text-foreground">Cursor</h3>
				<CodeBlock
					code={JSON.stringify({ mcpServers: { payrouters: { url: mcpUrl } } }, null, 2)}
					caption=".cursor/mcp.json"
					testid="snippet-cursor"
				/>
			</div>

			<div class="flex flex-col gap-2" data-testid="client-vscode">
				<h3 class="text-label text-foreground">VS Code</h3>
				<CodeBlock
					code={JSON.stringify({ servers: { payrouters: { type: 'http', url: mcpUrl } } }, null, 2)}
					caption=".vscode/mcp.json"
					testid="snippet-vscode"
				/>
			</div>
		{/if}
	</section>

	<section
		class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
		aria-labelledby="integrations-capabilities-title"
		data-testid="integrations-capabilities"
	>
		<h2 id="integrations-capabilities-title" class="text-card-title">What an assistant can do</h2>
		<ul class="flex flex-col gap-3">
			<li class="flex flex-col gap-1">
				<Pill>{scopeLabels.read}</Pill>
				<p class="text-body text-foreground">
					Read products, provider accounts with secrets masked, payments, webhook events and
					deliveries, stats and the API docs. Always granted.
				</p>
			</li>
			<li class="flex flex-col gap-1">
				<Pill>{scopeLabels.write}</Pill>
				<p class="text-body text-foreground">
					Create and edit products, rotate webhook secrets, create and revoke API keys, create test
					payments, simulate, sync and retry deliveries. On by default; the user can untick it at
					consent.
				</p>
			</li>
			<li class="flex flex-col gap-1">
				<Pill tone="pending">{scopeLabels.live}</Pill>
				<p class="text-body text-foreground">
					Extends reading and changes to live payments, live keys and live data. Off unless the user
					ticks it. Without it every tool works on sandbox data only.
				</p>
			</li>
		</ul>
		<p class="text-helper text-muted-foreground">
			Never offered through MCP: provider credentials, archiving products, deleting provider
			accounts and changing fee schedules.
		</p>
	</section>

	<section
		class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
		aria-labelledby="integrations-connected-title"
		data-testid="integrations-connected"
	>
		<div class="flex flex-col gap-1">
			<h2 id="integrations-connected-title" class="text-card-title">Connected apps</h2>
			<p class="text-body text-muted-foreground">
				Every app you approved. Revoking one stops its access at once.
			</p>
		</div>

		{#if grants.error}
			<ErrorAlert
				error={grants.error}
				subject="connected apps"
				onretry={() => grants.load(() => adminApi.listOAuthGrants())}
			/>
		{:else if !grants.data}
			<div class="flex flex-col divide-y divide-border" aria-busy="true">
				{#each [0, 1] as i (i)}
					<div class="flex flex-col gap-2 py-3">
						<Skeleton class="h-4 w-40" />
						<Skeleton class="h-4 w-64" />
					</div>
				{/each}
			</div>
		{:else if connections.length === 0}
			<EmptyState
				icon={Plug}
				title="No connected apps"
				description="Apps appear here after you approve them on the consent screen."
				dashed
			/>
		{:else}
			<ul class="flex flex-col divide-y divide-border" data-testid="connection-list">
				{#each connections as grant (grant.id)}
					<li
						class="flex flex-col gap-3 py-4 first:pt-0 last:pb-0 md:flex-row md:items-center md:justify-between"
						data-testid="connection-row"
					>
						<div class="flex min-w-0 flex-col gap-2">
							<p class="text-card-title break-words">{grant.client_name}</p>
							<div class="flex flex-wrap gap-1.5">
								{#each grant.scopes as key (key)}
									<Pill tone={key === 'live' ? 'pending' : 'neutral'}>{scopeLabels[key]}</Pill>
								{/each}
							</div>
							<p class="text-helper text-muted-foreground">
								Created {formatDateTime(grant.created_at)} · Last used
								{grant.last_used_at ? formatDateTime(grant.last_used_at) : 'Never'}
							</p>
						</div>
						<Button
							variant="outline"
							class="min-h-touch w-full md:w-auto"
							aria-label={`Revoke ${grant.client_name}`}
							onclick={() => askRevoke(grant)}
							data-testid="revoke-connection"
						>
							Revoke
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<ConfirmDialog
	bind:open={revokeOpen}
	testid="dialog-revoke-connection"
	title={revokeTarget ? `Revoke ${revokeTarget.client_name}?` : 'Revoke connection?'}
	description="The app loses access at once. Its access and refresh tokens stop working."
	consequences={[
		'Requests from the app are refused from now on.',
		'Reconnecting needs a new approval.'
	]}
	confirmLabel="Revoke"
	destructive
	onconfirm={revoke}
/>
