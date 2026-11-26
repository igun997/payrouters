<script lang="ts">
	import type { Snippet } from 'svelte';
	import Info from '@lucide/svelte/icons/info';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import CircleAlert from '@lucide/svelte/icons/circle-alert';
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import { cn } from '#lib/utils';

	/** Persistent message inside content. Toasts (svelte-sonner) are for transient feedback. */
	let {
		tone = 'info',
		title,
		children,
		action,
		class: className
	}: {
		tone?: 'info' | 'warn' | 'error' | 'success';
		title?: string;
		children?: Snippet;
		action?: Snippet;
		class?: string;
	} = $props();

	const tones = {
		info: { box: 'bg-secondary text-muted-foreground', title: 'text-foreground', icon: Info },
		warn: {
			box: 'bg-pending text-pending-foreground',
			title: 'text-pending-foreground',
			icon: TriangleAlert
		},
		error: {
			box: 'bg-failed text-failed-foreground',
			title: 'text-failed-foreground',
			icon: CircleAlert
		},
		success: {
			box: 'bg-success text-success-foreground',
			title: 'text-success-foreground',
			icon: CircleCheck
		}
	};
	const t = $derived(tones[tone]);
	const Icon = $derived(t.icon);
</script>

<div
	role={tone === 'error' ? 'alert' : 'status'}
	data-testid="banner"
	data-tone={tone}
	class={cn('flex w-full items-start gap-3 rounded-md p-3.5', t.box, className)}
>
	<Icon class="mt-0.5 size-[18px] shrink-0" />
	<div class="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
		{#if title}<span class={cn('font-semibold', t.title)}>{title}</span>{/if}
		{#if children}<span>{@render children()}</span>{/if}
	</div>
	{#if action}<div class="shrink-0">{@render action()}</div>{/if}
</div>
