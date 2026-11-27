<script lang="ts">
	/* Choose-method state: QRIS first, then bank transfer rows. The page owns the request; this only renders. */
	import MethodRow from './method-row.svelte';
	import type { Method, SnapMethod } from '#lib/api/types';

	let {
		methods,
		connecting,
		failed,
		onpick
	}: {
		methods: SnapMethod[];
		/** Method whose request is in flight. */
		connecting: Method | null;
		/** Last method the provider refused to start. */
		failed: { method: Method } | null;
		onpick: (code: Method) => void;
	} = $props();

	const groups = $derived(
		[
			{ title: 'Scan to pay', items: methods.filter((m) => m.group === 'qris') },
			{ title: 'Bank transfer', items: methods.filter((m) => m.group === 'virtual_account') }
		].filter((group) => group.items.length > 0)
	);

	function rowState(code: Method) {
		if (connecting === code) return 'connecting' as const;
		if (failed?.method === code) return 'error' as const;
		return 'idle' as const;
	}
</script>

<section class="flex flex-col gap-section" data-testid="snap-methods">
	<header class="flex flex-col gap-1">
		<h1 class="text-page-title font-extrabold">Choose how to pay</h1>
		<p class="text-body text-muted-foreground">
			You will see the account or QR code on the next step
		</p>
	</header>

	{#each groups as group (group.title)}
		<div class="flex flex-col gap-3 sm:gap-2">
			<h2 class="text-micro text-muted-foreground">{group.title}</h2>
			<ul class="flex flex-col gap-3 sm:gap-2">
				{#each group.items as method (method.code)}
					<li class="overflow-hidden rounded-lg border border-border bg-card">
						<MethodRow
							{method}
							state={rowState(method.code)}
							disabled={connecting !== null}
							{onpick}
						/>
					</li>
				{/each}
			</ul>
		</div>
	{:else}
		<p class="py-4 text-center text-body text-muted-foreground">
			No payment methods are available for this order right now.
		</p>
	{/each}
</section>
