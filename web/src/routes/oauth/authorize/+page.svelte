<script lang="ts">
	// Consent screen for an MCP client (docs/mcp.md, "Authorization and consent"). Standalone card,
	// outside the admin shell. The dashboard session approves the request; the server validates the
	// query string and decides where the browser goes next. This page only ever navigates to the
	// redirect_url the server returned after a decision.
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { adminApi } from '#lib/api/admin';
	import { ApiError } from '#lib/api/client';
	import type { OAuthAuthorizePreview, OAuthScope } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import BrandMark from '#lib/components/app/brand-mark.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Skeleton } from '#lib/components/ui/skeleton';

	const preview = new Resource<OAuthAuthorizePreview>();
	const search = $derived(page.url.search);

	// Load (and reload if the query changes) the request the client asked for.
	$effect(() => {
		const query = search;
		void preview.load(() => adminApi.oauthAuthorizePreview(query));
	});

	// 401 already sends the browser to sign-in (the client layer does that); keep the skeleton up.
	function isUnauthorized(error: unknown): boolean {
		return error instanceof ApiError && error.status === 401;
	}
	const loadFailed = $derived(preview.error !== null && !isUnauthorized(preview.error));

	const scopes = $derived(preview.data?.scopes ?? []);
	const standard = $derived(scopes.filter((scope) => scope.key !== 'live'));
	const live = $derived(scopes.filter((scope) => scope.key === 'live'));
	const clientName = $derived(preview.data?.client.client_name ?? 'the app');

	// Checked state per scope key. Locked scopes are always granted.
	let selected = $state<Record<string, boolean>>({});
	$effect(() => {
		const list: OAuthScope[] = preview.data?.scopes ?? [];
		if (list.length === 0) return;
		selected = Object.fromEntries(list.map((scope) => [scope.key, scope.granted || scope.locked]));
	});

	let deciding = $state<'allow' | 'deny' | null>(null);
	let returning = $state(false);
	let decideError = $state<unknown>(null);
	const busy = $derived(deciding !== null || returning);

	// Allow and Deny send the final scope list; a second press while one request is in flight is ignored.
	async function decide(decision: 'allow' | 'deny') {
		if (busy || !preview.data) return;
		deciding = decision;
		decideError = null;
		const granted = scopes
			.filter((scope) => scope.locked || selected[scope.key])
			.map((scope) => scope.key);
		try {
			const result = await adminApi.oauthAuthorizeDecide(search, {
				decision,
				scope: granted.join(' ')
			});
			returning = true;
			// The server built this address from the validated redirect_uri and the state.
			window.location.assign(result.redirect_url);
		} catch (error) {
			deciding = null;
			if (!isUnauthorized(error)) decideError = error;
		}
	}

	const failure = $derived(loadFailed || decideError !== null);
</script>

<svelte:head>
	<title>Authorize access · PayRouters</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<main
	class="flex min-h-svh flex-col bg-background sm:items-center sm:justify-center sm:p-page"
	data-testid="page-oauth-authorize"
>
	<div
		class="mx-auto flex w-full max-w-[400px] flex-1 flex-col gap-6 p-page sm:flex-none sm:rounded-xl sm:border sm:border-border sm:bg-card sm:p-dialog sm:shadow-sm"
	>
		<BrandMark tile size={22} />

		{#if returning}
			<div class="flex flex-col gap-2" role="status" data-testid="oauth-returning">
				<h1 class="text-result-title" data-testid="page-title">
					Returning to {clientName}…
				</h1>
				<p class="text-body text-muted-foreground">You can close this tab if nothing happens.</p>
			</div>
		{:else if failure}
			<div class="flex flex-col gap-4" data-testid="oauth-error">
				<h1 class="text-result-title" data-testid="page-title">This request cannot be approved</h1>
				{#if loadFailed}
					<ErrorAlert error={preview.error} subject="this request" />
				{:else}
					<Banner tone="error" title="Request failed">
						{#if decideError instanceof ApiError && decideError.code === 'http_error'}
							The server returned {decideError.status}.
						{:else if decideError instanceof Error}
							{decideError.message}
						{:else}
							Something went wrong. Please try again.
						{/if}
					</Banner>
				{/if}
				<a
					href={resolve('/(admin)')}
					class="text-label text-foreground underline underline-offset-4"
					data-testid="oauth-dashboard-link"
				>
					Back to the dashboard
				</a>
			</div>
		{:else if !preview.data}
			<div class="flex flex-col gap-4" data-testid="oauth-loading" aria-busy="true">
				<Skeleton class="h-7 w-3/4" />
				<Skeleton class="h-4 w-1/2" />
				<Skeleton class="h-16 w-full" />
				<Skeleton class="h-16 w-full" />
				<Skeleton class="h-control w-full" />
			</div>
		{:else}
			<div class="flex flex-col gap-4">
				<div class="flex flex-col gap-2">
					<h1 class="text-result-title" data-testid="page-title">
						Connect {preview.data.client.client_name} to PayRouters
					</h1>
					<p class="text-body text-muted-foreground">
						After you decide, you return to
						<span class="font-mono text-foreground" data-testid="oauth-redirect-host">
							{preview.data.redirect_host}
						</span>.
					</p>
				</div>

				<fieldset class="flex flex-col gap-3" data-testid="oauth-scopes">
					<legend class="text-label text-foreground">Permissions</legend>
					{#each standard as scope (scope.key)}
						<label
							class="flex min-h-touch cursor-pointer items-start gap-3 rounded-md border border-border p-3.5 has-disabled:cursor-default"
						>
							<input
								type="checkbox"
								class="mt-0.5 size-5 shrink-0 accent-primary"
								name="scope"
								value={scope.key}
								bind:checked={selected[scope.key]}
								disabled={scope.locked || deciding !== null}
								data-testid={`scope-${scope.key}`}
							/>
							<span class="flex min-w-0 flex-col gap-0.5">
								<span class="text-card-title">{scope.label}</span>
								<span class="text-helper text-muted-foreground">
									{scope.description}{scope.locked ? ' Always on.' : ''}
								</span>
							</span>
						</label>
					{/each}
					{#each live as scope (scope.key)}
						<label
							class="flex min-h-touch cursor-pointer items-start gap-3 rounded-md bg-pending p-3.5 text-pending-foreground has-disabled:cursor-default"
						>
							<input
								type="checkbox"
								class="mt-0.5 size-5 shrink-0 accent-primary"
								name="scope"
								value={scope.key}
								bind:checked={selected[scope.key]}
								disabled={scope.locked || deciding !== null}
								data-testid={`scope-${scope.key}`}
							/>
							<span class="flex min-w-0 flex-col gap-0.5">
								<span class="text-card-title">{scope.label}</span>
								<span class="text-helper">{scope.description}</span>
							</span>
						</label>
					{/each}
				</fieldset>

				<p class="text-helper text-muted-foreground">
					You can revoke this connection later under Integrations.
				</p>

				<div class="flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
					<Button
						variant="outline"
						class="min-h-touch w-full sm:w-auto"
						disabled={busy}
						onclick={() => decide('deny')}
						data-testid="oauth-deny"
					>
						Deny
					</Button>
					<Button
						class="min-h-touch w-full sm:w-auto"
						disabled={busy}
						onclick={() => decide('allow')}
						data-testid="oauth-allow"
					>
						Allow
					</Button>
				</div>
			</div>
		{/if}
	</div>
</main>
