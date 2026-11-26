<script lang="ts">
	// Admin frame: 248px sidebar from lg, 72px icon rail at md, bottom tab bar below md. The main
	// column holds the top bar, the Live banner and the scrolling page content. The More panel
	// replaces the page area on phones and closes itself on navigation (it is tied to the path).
	import type { Snippet } from 'svelte';
	import BrandMark from './brand-mark.svelte';
	import AppShellTopBar from './app-shell-topbar.svelte';
	import AppShellTabBar from './app-shell-tabbar.svelte';
	import AppShellMore from './app-shell-more.svelte';
	import LiveBanner from './live-banner.svelte';
	import { sidebarNav, isActive } from './app-shell-nav';
	import LogOut from '@lucide/svelte/icons/log-out';
	import { cn } from '#lib/utils';

	let {
		path,
		title,
		subtitle = null,
		showRange = false,
		onsignout,
		children
	}: {
		path: string;
		title: string;
		subtitle?: string | null;
		showRange?: boolean;
		onsignout: () => void;
		children: Snippet;
	} = $props();

	/** The path the More panel was opened on; it is open only while the path is unchanged. */
	let moreAt = $state<string | null>(null);
	const moreOpen = $derived(moreAt === path);
</script>

<div class="flex h-svh overflow-hidden bg-background text-foreground">
	<aside
		data-testid="sidebar"
		class="hidden shrink-0 flex-col border-r border-sidebar-border bg-sidebar md:flex md:w-[72px] lg:w-[248px]"
	>
		<div class="flex items-center gap-3 px-4 pt-5 pb-4 md:justify-center lg:justify-start lg:px-5">
			<BrandMark tile size={22} />
			<div class="hidden min-w-0 flex-col lg:flex">
				<span class="text-card-title">PayRouters</span>
				<span class="text-helper text-muted-foreground">Admin dashboard</span>
			</div>
		</div>
		<p class="hidden px-5 pt-3 pb-2 text-micro text-muted-foreground lg:block">Dashboard</p>
		<nav aria-label="Sections" class="flex flex-1 flex-col gap-1 overflow-y-auto px-3">
			<!-- eslint-disable svelte/no-navigation-without-resolve -- hrefs come from app-shell-nav.ts, already resolved -->
			{#each sidebarNav as item (item.href)}
				{@const active = isActive(path, item.href)}
				{@const Icon = item.icon}
				<a
					href={item.href}
					aria-label={item.label}
					title={item.label}
					aria-current={active ? 'page' : undefined}
					class={cn(
						'flex h-11 items-center gap-3 rounded-md px-3 text-label transition-colors md:justify-center lg:justify-start',
						active
							? 'bg-sidebar-accent font-semibold text-foreground'
							: 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground'
					)}
				>
					<Icon class="size-5 shrink-0" />
					<span class="hidden lg:inline">{item.label}</span>
				</a>
			{/each}
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		</nav>
		<div class="px-3 py-4">
			<button
				type="button"
				onclick={onsignout}
				aria-label="Sign out"
				class="flex h-11 w-full items-center gap-3 rounded-md px-3 text-label text-muted-foreground hover:bg-sidebar-accent hover:text-foreground md:justify-center lg:justify-start"
			>
				<LogOut class="size-5 shrink-0" />
				<span class="hidden lg:inline">Sign out</span>
			</button>
		</div>
	</aside>

	<div class="flex min-w-0 flex-1 flex-col">
		{#if moreOpen}
			<AppShellMore onclose={() => (moreAt = null)} {onsignout} />
		{:else}
			<AppShellTopBar {title} {subtitle} {showRange} />
			<LiveBanner />
			<main class="flex min-h-0 flex-1 flex-col overflow-y-auto">
				<div
					class="flex flex-col gap-section p-page pb-[calc(5rem+env(safe-area-inset-bottom))] md:pb-page 2xl:mx-auto 2xl:w-full 2xl:max-w-[1280px]"
					data-testid="shell-content"
				>
					{@render children()}
				</div>
			</main>
		{/if}
	</div>

	<AppShellTabBar {path} {moreOpen} onmore={() => (moreAt = moreOpen ? null : path)} />
</div>
