<script lang="ts">
	import type { PaymentStatus } from '#lib/api/types';
	import { STATUS_LABEL } from '#lib/format';
	import { cn } from '#lib/utils';

	let { status, class: className }: { status: PaymentStatus; class?: string } = $props();

	const tone: Record<PaymentStatus, { box: string; dot: string }> = {
		paid: { box: 'bg-success text-success-foreground', dot: 'bg-signal' },
		pending: { box: 'bg-pending text-pending-foreground', dot: 'bg-pending-dot' },
		expired: { box: 'bg-expired text-expired-foreground', dot: 'bg-subtle-foreground' },
		failed: { box: 'bg-failed text-failed-foreground', dot: 'bg-failed-foreground' },
		cancelled: { box: 'bg-expired text-expired-foreground', dot: 'bg-subtle-foreground' }
	};
</script>

<span
	data-testid="status-badge"
	data-status={status}
	class={cn(
		'inline-flex h-[26px] w-fit shrink-0 items-center gap-1.5 rounded-full px-2.5 text-helper font-semibold',
		tone[status].box,
		className
	)}
>
	<span class={cn('size-1.5 rounded-full', tone[status].dot)}></span>
	{STATUS_LABEL[status]}
</span>
