<script lang="ts">
	// Phone "More" screen: session, the secondary sections (Products, Webhooks, Playground, API docs,
	// Integrations), data mode, theme, sign out and version. Replaces the page area while open; the
	// tab bar stays.
	import { resolve } from '$app/paths';
	import { userPrefersMode, setMode } from 'mode-watcher';
	import BrandMark from './brand-mark.svelte';
	import ModeToggle from './mode-toggle.svelte';
	import SelectField from './select-field.svelte';
	import { Button } from '#lib/components/ui/button';
	import ShieldCheck from '@lucide/svelte/icons/shield-check';
	import Package from '@lucide/svelte/icons/package';
	import Webhook from '@lucide/svelte/icons/webhook';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import BookOpen from '@lucide/svelte/icons/book-open';
	import Plug from '@lucide/svelte/icons/plug';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import LogOut from '@lucide/svelte/icons/log-out';

	let { onclose, onsignout }: { onclose: () => void; onsignout: () => void } = $props();

	type Row = { href: string; label: string; hint: string; icon: typeof Package };
	const manage: Row[] = [
		{
			href: resolve('/(admin)/products'),
			label: 'Products',
			hint: 'API keys and webhook secrets',
			icon: Package
		},
		{
			href: resolve('/(admin)/webhooks'),
			label: 'Webhooks',
			hint: 'Inbound events and outbound deliveries',
			icon: Webhook
		},
		{
			href: resolve('/(admin)/playground'),
			label: 'Playground',
			hint: 'Create a sandbox payment',
			icon: FlaskConical
		},
		{
			href: resolve('/(admin)/docs'),
			label: 'API docs',
			hint: 'Create payments, webhooks and errors',
			icon: BookOpen
		},
		{
			href: resolve('/(admin)/integrations'),
			label: 'Integrations',
			hint: 'Connect AI assistants',
			icon: Plug
		}
	];

	const themes = [
		{ value: 'system', label: 'System' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' }
	];

	function chooseTheme(value: string) {
		if (value === 'system' || value === 'light' || value === 'dark') setMode(value);
	}
</script>

<div class="flex min-h-0 flex-1 flex-col" data-testid="page-more">
	<header
		class="flex shrink-0 items-center gap-3 border-b border-border bg-background px-page py-3"
	>
		<BrandMark tile size={22} />
		<p class="min-w-0 flex-1 truncate text-page-title">More</p>
		<ModeToggle />
	</header>

	<div
		class="flex min-h-0 flex-1 flex-col gap-section overflow-y-auto p-page pb-[calc(5rem+env(safe-area-inset-bottom))]"
	>
		<section class="flex items-center gap-3 rounded-lg border border-border bg-card p-card">
			<span
				class="flex size-10 shrink-0 items-center justify-center rounded-full bg-success text-success-foreground"
			>
				<ShieldCheck class="size-5" />
			</span>
			<div class="flex min-w-0 flex-col gap-0.5">
				<p class="text-card-title">Admin session</p>
				<p class="text-helper text-muted-foreground">Signed in</p>
			</div>
		</section>

		<section class="flex flex-col gap-2">
			<p class="text-micro text-muted-foreground">Manage</p>
			<ul class="flex flex-col overflow-hidden rounded-lg border border-border bg-card">
				{#each manage as row, i (row.href)}
					{@const Icon = row.icon}
					<li class={i > 0 ? 'border-t border-border' : ''}>
						<a
							href={row.href}
							class="flex min-h-touch items-center gap-3 px-card py-3.5"
							onclick={onclose}
						>
							<span
								class="flex size-9 shrink-0 items-center justify-center rounded-md bg-secondary"
							>
								<Icon class="size-4" />
							</span>
							<span class="flex min-w-0 flex-1 flex-col">
								<span class="text-card-title">{row.label}</span>
								<span class="text-helper text-muted-foreground">{row.hint}</span>
							</span>
							<ChevronRight class="size-4 text-muted-foreground" />
						</a>
					</li>
				{/each}
			</ul>
		</section>

		<section class="flex flex-col gap-2">
			<p class="text-micro text-muted-foreground">Preferences</p>
			<div
				class="flex flex-col divide-y divide-border overflow-hidden rounded-lg border border-border bg-card"
			>
				<div class="flex flex-col gap-3 p-card">
					<div class="flex flex-col gap-0.5">
						<p class="text-card-title">Data mode</p>
						<p class="text-helper text-muted-foreground">
							Sandbox shows test data. Live shows real payments.
						</p>
					</div>
					<ModeToggle variant="segmented" />
				</div>
				<div class="flex items-center justify-between gap-3 p-card">
					<p class="text-card-title">Theme</p>
					<SelectField
						bind:value={() => userPrefersMode.current, chooseTheme}
						options={themes}
						label="Theme"
						class="w-32"
					/>
				</div>
			</div>
		</section>

		<Button variant="outline" class="w-full" onclick={onsignout}>
			<LogOut data-icon="inline-start" />
			Sign out
		</Button>

		<p class="text-center text-helper text-muted-foreground">PayRouters</p>
	</div>
</div>
