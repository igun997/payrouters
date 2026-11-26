<script lang="ts">
	import type { Snippet } from 'svelte';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { clearToken } from '#lib/api/client';
	import { shell } from '#lib/state/shell.svelte';
	import { session } from '#lib/state/session.svelte';
	import AppShell from '#lib/components/app/app-shell.svelte';
	import SessionExpiredDialog from '#lib/components/app/session-expired-dialog.svelte';
	import { fallbackTitle } from '#lib/components/app/app-shell-nav';

	let { children }: { children: Snippet } = $props();

	onMount(() => {
		session.inShell = true;
		return () => {
			session.inShell = false;
		};
	});

	const title = $derived(shell.title || fallbackTitle(page.url.pathname));

	function signOut() {
		clearToken();
		session.reset();
		void goto(resolve('/login'), { replaceState: true });
	}
</script>

<svelte:head>
	<title>{title} | PayRouters</title>
</svelte:head>

<AppShell
	path={page.url.pathname}
	{title}
	subtitle={shell.title ? shell.subtitle : null}
	showRange={shell.rangeControl}
	onsignout={signOut}
>
	{@render children()}
</AppShell>

<SessionExpiredDialog />
