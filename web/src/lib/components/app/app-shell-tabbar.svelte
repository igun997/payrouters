<script lang="ts">
	// Phone bottom tab bar: Overview, Payments, Cash-ins, Providers and More. Hidden from md up,
	// where the sidebar or icon rail takes over. More is active while its panel is open and on the
	// pages it leads to (Products, Webhooks, Playground).
	import { tabNav, isActive } from './app-shell-nav';
	import Ellipsis from '@lucide/svelte/icons/ellipsis';
	import { cn } from '#lib/utils';

	let {
		path,
		moreOpen = false,
		onmore
	}: { path: string; moreOpen?: boolean; onmore: () => void } = $props();

	const onTab = $derived(tabNav.some((item) => isActive(path, item.href)));
	const moreActive = $derived(moreOpen || !onTab);
</script>

<nav
	aria-label="Main"
	data-testid="tab-bar"
	class="fixed inset-x-0 bottom-0 z-30 flex border-t border-border bg-card pb-[env(safe-area-inset-bottom)] md:hidden"
>
	<!-- eslint-disable svelte/no-navigation-without-resolve -- hrefs come from app-shell-nav.ts, already resolved -->
	{#each tabNav as item (item.href)}
		{@const active = !moreOpen && isActive(path, item.href)}
		{@const Icon = item.icon}
		<a
			href={item.href}
			aria-current={active ? 'page' : undefined}
			class:text-foreground={active}
			class="flex min-h-16 min-w-0 flex-1 flex-col items-center justify-center gap-0.5 text-tab text-muted-foreground"
		>
			<span
				class={cn(
					'flex h-8 w-14 items-center justify-center rounded-full',
					active && 'bg-secondary'
				)}
			>
				<Icon class="size-5" />
			</span>
			<span class="max-w-full truncate">{item.label}</span>
		</a>
	{/each}
	<!-- eslint-enable svelte/no-navigation-without-resolve -->
	<button
		type="button"
		aria-expanded={moreOpen}
		aria-current={moreActive && !moreOpen ? 'page' : undefined}
		data-testid="tab-more"
		class={cn(
			'flex min-h-16 min-w-0 flex-1 flex-col items-center justify-center gap-0.5 text-tab text-muted-foreground',
			moreActive && 'text-foreground'
		)}
		onclick={onmore}
	>
		<span
			class={cn(
				'flex h-8 w-14 items-center justify-center rounded-full',
				moreActive && 'bg-secondary'
			)}
		>
			<Ellipsis class="size-5" />
		</span>
		<span class="max-w-full truncate">More</span>
	</button>
</nav>
