<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { adminApi } from '#lib/api/admin';
	import { ApiError, errorMessage, setToken } from '#lib/api/client';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import BrandMark from '#lib/components/app/brand-mark.svelte';
	import FormField from '#lib/components/app/form-field.svelte';
	import { session } from '#lib/state/session.svelte';
	import Eye from '@lucide/svelte/icons/eye';
	import EyeOff from '@lucide/svelte/icons/eye-off';
	import LogIn from '@lucide/svelte/icons/log-in';

	const REJECTED = 'That token was not accepted';

	let token = $state('');
	let show = $state(false);
	let submitting = $state(false);
	let error = $state<string | null>(null);
	// Set when the server refuses attempts from this client; the form stays closed until it lapses.
	let lockedUntil = $state<number | null>(null);
	let now = $state(Date.now());
	const lockSeconds = $derived(
		lockedUntil === null ? 0 : Math.max(0, Math.ceil((lockedUntil - now) / 1000))
	);
	const locked = $derived(lockSeconds > 0);
	const message = $derived(
		locked ? `Too many attempts. Try again in ${lockSeconds} seconds.` : error
	);

	$effect(() => {
		if (!locked) return;
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// Only same-app paths are accepted as the post-login destination.
	function safeNext(value: string | null): string {
		if (!value || !value.startsWith('/') || value.startsWith('//') || value.startsWith('/login')) {
			return resolve('/(admin)');
		}
		return value;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const candidate = token.trim();
		if (!candidate || submitting || locked) return;
		submitting = true;
		error = null;
		try {
			// Validate the token against the server before storing it.
			await adminApi.me({ token: candidate });
			setToken(candidate);
			session.reset();
			// safeNext() only returns same-app paths, checked above.
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			await goto(safeNext(page.url.searchParams.get('next')), { replaceState: true });
		} catch (err) {
			if (err instanceof ApiError && err.status === 429 && err.retryAfter !== null) {
				now = Date.now();
				lockedUntil = now + err.retryAfter * 1000;
			} else {
				error = err instanceof ApiError && err.status === 401 ? REJECTED : errorMessage(err);
			}
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Sign in · PayRouters</title>
</svelte:head>

<main
	class="flex min-h-svh flex-col bg-background sm:items-center sm:justify-center sm:p-page"
	data-testid="page-login"
>
	<div
		class="mx-auto flex w-full max-w-[400px] flex-1 flex-col gap-6 p-page sm:flex-none sm:rounded-xl sm:border sm:border-border sm:bg-card sm:p-dialog sm:shadow-sm"
	>
		<div class="flex flex-col gap-4">
			<BrandMark tile size={22} />
			<div class="flex flex-col gap-2">
				<h1 class="text-result-title" data-testid="page-title">PayRouters admin</h1>
				<p class="text-body text-muted-foreground">
					Sign in with the admin token to manage products, payments and providers.
				</p>
			</div>
		</div>

		<form class="flex flex-col gap-5" onsubmit={submit} novalidate>
			<FormField id="admin-token" label="Admin token" error={message}>
				<div class="relative">
					<Input
						id="admin-token"
						type={show ? 'text' : 'password'}
						autocomplete="off"
						spellcheck={false}
						bind:value={token}
						class="h-control pr-12"
						aria-invalid={message ? true : undefined}
						aria-describedby={message ? 'admin-token-error' : undefined}
						data-testid="token-input"
					/>
					<button
						type="button"
						class="absolute top-1/2 right-1 inline-flex size-11 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground hover:text-foreground md:size-9"
						aria-label={show ? 'Hide token' : 'Show token'}
						aria-pressed={show}
						onclick={() => (show = !show)}
					>
						{#if show}<EyeOff class="size-[18px]" />{:else}<Eye class="size-[18px]" />{/if}
					</button>
				</div>
			</FormField>

			<Button
				type="submit"
				class="w-full"
				disabled={submitting || locked || !token.trim()}
				data-testid="sign-in"
			>
				<LogIn data-icon="inline-start" />
				{submitting ? 'Checking…' : 'Sign in'}
			</Button>
		</form>

		<p class="text-helper text-muted-foreground">
			The token stays in this browser until you sign out.
		</p>
	</div>
</main>
