<script lang="ts">
	/** First-run guide on an empty Products page. Each step is done, next or to do. */
	import type { ChecklistStep } from '#lib/checklist';
	import Pill from '#lib/components/app/pill.svelte';
	import { Button } from '#lib/components/ui/button';
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import Circle from '@lucide/svelte/icons/circle';
	import CircleDot from '@lucide/svelte/icons/circle-dot';
	import { cn } from '#lib/utils';

	let {
		steps,
		title,
		description
	}: { steps: ChecklistStep[]; title: string; description: string } = $props();

	const done = $derived(steps.filter((s) => s.status === 'done').length);
	const pillLabel = { done: 'Done', next: 'Next', todo: 'To do' } as const;
	const pillTone = { done: 'success', next: 'sandbox', todo: 'neutral' } as const;
</script>

<section class="flex flex-col rounded-lg border bg-card" data-testid="onboarding-checklist">
	<header class="flex flex-wrap items-start justify-between gap-3 border-b px-card py-4">
		<div class="flex min-w-0 flex-col gap-1">
			<h2 class="text-card-title text-foreground">{title}</h2>
			<p class="text-helper text-muted-foreground">{description}</p>
		</div>
		<span class="tabular shrink-0 text-helper text-muted-foreground">
			{done} of {steps.length} done
		</span>
	</header>
	<ol class="flex flex-col">
		{#each steps as step, i (step.title)}
			<li
				class={cn(
					'flex flex-col gap-3 px-card py-4 sm:flex-row sm:items-center sm:justify-between',
					i > 0 && 'border-t'
				)}
				data-status={step.status}
			>
				<div class="flex min-w-0 items-start gap-3">
					{#if step.status === 'done'}
						<CircleCheck class="mt-0.5 size-5 shrink-0 text-primary" />
					{:else if step.status === 'next'}
						<CircleDot class="mt-0.5 size-5 shrink-0 text-primary" />
					{:else}
						<Circle class="mt-0.5 size-5 shrink-0 text-subtle-foreground" />
					{/if}
					<div class="flex min-w-0 flex-col gap-1">
						<div class="flex flex-wrap items-center gap-2">
							<span class="text-helper text-muted-foreground">Step {i + 1}</span>
							<Pill tone={pillTone[step.status]}>{pillLabel[step.status]}</Pill>
						</div>
						<span class="text-card-title text-foreground">{step.title}</span>
						<span class="text-body text-muted-foreground">{step.helper}</span>
					</div>
				</div>
				{#if step.href}
					<Button variant="outline" class="shrink-0" href={step.href}>{step.actionLabel}</Button>
				{:else}
					<Button
						variant={step.status === 'next' ? 'default' : 'outline'}
						class="shrink-0"
						disabled={step.status !== 'next'}
						onclick={step.onAction}
					>
						{step.actionLabel}
					</Button>
				{/if}
			</li>
		{/each}
	</ol>
</section>
