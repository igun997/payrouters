<script lang="ts">
	/* Payment deadline in Asia/Jakarta, e.g. "Pay before 27 Nov 2026, 19.48 WIB". */
	import Clock from '@lucide/svelte/icons/clock';
	import { formatDateTime } from '#lib/format';

	let { expiresAt }: { expiresAt: string } = $props();
</script>

<p class="flex items-center gap-1.5 text-helper text-muted-foreground">
	<Clock class="size-3.5 shrink-0" aria-hidden="true" />
	<span>Pay before {formatDateTime(expiresAt)}</span>
</p>
