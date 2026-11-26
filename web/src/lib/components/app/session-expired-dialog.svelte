<script lang="ts">
	// Shown over the current page when an API call answers 401 (session.markExpired). Sign in again
	// clears the stored token and returns to the same page after login.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { clearToken } from '#lib/api/client';
	import { session } from '#lib/state/session.svelte';
	import ResponsiveDialog from './responsive-dialog.svelte';
	import { Button } from '#lib/components/ui/button';
	import LogIn from '@lucide/svelte/icons/log-in';

	function signInAgain() {
		const next = page.url.pathname + page.url.search;
		clearToken();
		session.reset();
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- resolve() supplies the path; only the next query is appended
		void goto(`${resolve('/login')}?next=${encodeURIComponent(next)}`, { replaceState: true });
	}
</script>

<ResponsiveDialog
	bind:open={session.expired}
	title="Your session ended"
	description="Your admin session has expired. Sign in again to return to this page."
	testid="dialog-session-expired"
>
	<span
		class="flex size-11 items-center justify-center rounded-md bg-pending text-pending-foreground"
	>
		<LogIn class="size-5" />
	</span>
	{#snippet footer()}
		<Button class="w-full" onclick={signInAgain}>
			<LogIn data-icon="inline-start" />
			Sign in again
		</Button>
	{/snippet}
</ResponsiveDialog>
