<script lang="ts" generics="T extends string">
	/**
	 * A single choice shown as selectable cards (native radios underneath, so keyboard and screen
	 * readers work unchanged). Used for the key mode and the provider mode.
	 */
	import { cn } from '#lib/utils';

	type Option = { value: T; label: string; description?: string; disabled?: boolean };

	let {
		name,
		legend,
		value = $bindable(),
		options,
		testid
	}: {
		name: string;
		legend: string;
		value: T;
		options: Option[];
		testid?: string;
	} = $props();
</script>

<fieldset class="flex min-w-0 flex-col gap-2" data-testid={testid}>
	<legend class="mb-1.5 text-label text-foreground">{legend}</legend>
	<div class="grid gap-2 sm:grid-cols-2">
		{#each options as option (option.value)}
			<label
				class={cn(
					'flex min-h-touch cursor-pointer items-start gap-3 rounded-md border border-input bg-card p-3.5 transition-colors',
					'has-checked:border-primary has-checked:ring-1 has-checked:ring-primary',
					'has-focus-visible:ring-3 has-focus-visible:ring-ring/50',
					option.disabled && 'cursor-not-allowed opacity-60'
				)}
			>
				<input
					type="radio"
					class="peer sr-only"
					{name}
					value={option.value}
					checked={value === option.value}
					disabled={option.disabled}
					onchange={() => (value = option.value)}
				/>
				<span
					aria-hidden="true"
					class="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border-[1.5px] border-input peer-checked:border-primary [&>span]:opacity-0 peer-checked:[&>span]:opacity-100"
				>
					<span class="size-2 rounded-full bg-primary"></span>
				</span>
				<span class="flex min-w-0 flex-col gap-0.5">
					<span class="text-label text-foreground">{option.label}</span>
					{#if option.description}
						<span class="text-helper text-muted-foreground">{option.description}</span>
					{/if}
				</span>
			</label>
		{/each}
	</div>
</fieldset>
