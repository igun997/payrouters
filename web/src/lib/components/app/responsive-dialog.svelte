<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Dialog from '#lib/components/ui/dialog';
	import * as Sheet from '#lib/components/ui/sheet';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import { cn } from '#lib/utils';

	/**
	 * Dialog from 640px (sm) up, bottom sheet below. `fullScreenOnPhone` turns the sheet into a
	 * full screen with a sticky footer (forms with 4+ fields). The body scrolls between a fixed
	 * header and footer.
	 */
	let {
		open = $bindable(false),
		title,
		description,
		size = 'md',
		fullScreenOnPhone = false,
		testid,
		children,
		footer
	}: {
		open?: boolean;
		title: string;
		description?: string;
		/** md = 480px, lg = 560px */
		size?: 'md' | 'lg';
		fullScreenOnPhone?: boolean;
		testid?: string;
		children: Snippet;
		footer?: Snippet;
	} = $props();

	const phone = new IsMobile(640);
	const width = $derived(size === 'lg' ? 'sm:max-w-[560px]' : 'sm:max-w-[480px]');
</script>

{#if phone.current}
	<Sheet.Root bind:open>
		<Sheet.Content
			side="bottom"
			showCloseButton={false}
			data-testid={testid}
			class={cn(
				'gap-0 p-0',
				fullScreenOnPhone
					? 'h-dvh max-h-dvh rounded-none'
					: 'max-h-[92dvh] rounded-t-xl border-border'
			)}
		>
			{#if !fullScreenOnPhone}
				<div class="flex justify-center pt-2.5">
					<div class="h-[5px] w-10 rounded-full bg-input"></div>
				</div>
			{/if}
			<div class="flex flex-col gap-1 px-page pe-16 pt-4 pb-3">
				<Sheet.Title class="text-dialog-title">{title}</Sheet.Title>
				{#if description}
					<Sheet.Description class="text-body text-muted-foreground">
						{description}
					</Sheet.Description>
				{/if}
			</div>
			<Sheet.Close
				class="absolute top-2 right-3 inline-flex size-11 items-center justify-center rounded-full bg-secondary text-foreground"
				aria-label="Close"
			>
				<svg
					viewBox="0 0 24 24"
					class="size-[18px]"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
				>
					<path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" />
				</svg>
			</Sheet.Close>
			<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-page pb-4">
				{@render children()}
			</div>
			{#if footer}
				<div
					class="flex flex-col gap-2.5 border-t bg-popover px-page pt-3 pb-[max(1.5rem,env(safe-area-inset-bottom))]"
				>
					{@render footer()}
				</div>
			{/if}
		</Sheet.Content>
	</Sheet.Root>
{:else}
	<Dialog.Root bind:open>
		<Dialog.Content
			showCloseButton={false}
			data-testid={testid}
			class={cn('gap-0 rounded-xl p-0', width)}
		>
			<div class="flex items-start justify-between gap-4 px-dialog pt-dialog pb-4">
				<div class="flex flex-col gap-1">
					<Dialog.Title class="text-dialog-title">{title}</Dialog.Title>
					{#if description}
						<Dialog.Description class="text-body text-muted-foreground">
							{description}
						</Dialog.Description>
					{/if}
				</div>
				<Dialog.Close
					class="inline-flex size-11 shrink-0 items-center justify-center rounded-full bg-secondary text-foreground"
					aria-label="Close"
				>
					<svg
						viewBox="0 0 24 24"
						class="size-[18px]"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
					>
						<path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" />
					</svg>
				</Dialog.Close>
			</div>
			<div class="flex max-h-[70svh] flex-col gap-4 overflow-y-auto px-dialog pb-dialog">
				{@render children()}
			</div>
			{#if footer}
				<div class="flex items-center justify-end gap-3 px-dialog pb-dialog">
					{@render footer()}
				</div>
			{/if}
		</Dialog.Content>
	</Dialog.Root>
{/if}
