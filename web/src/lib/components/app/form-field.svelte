<script lang="ts">
	import type { Snippet } from 'svelte';
	import CircleAlert from '@lucide/svelte/icons/circle-alert';
	import { cn } from '#lib/utils';

	/**
	 * Label above, helper or error below. Pass the control as children and give it `id={id}` and
	 * `aria-invalid={!!error}`. Error text replaces the helper (never colour alone). The control's
	 * `aria-describedby` is set here to point at the visible error or helper, so it is announced with
	 * the field.
	 */
	let {
		id,
		label,
		optional = false,
		required = false,
		helper,
		error,
		class: className,
		children
	}: {
		id: string;
		label: string;
		optional?: boolean;
		required?: boolean;
		helper?: string;
		error?: string | null;
		class?: string;
		children: Snippet;
	} = $props();

	$effect(() => {
		const control = document.getElementById(id);
		if (!control) return;
		const described = error ? `${id}-error` : helper ? `${id}-helper` : null;
		if (described) control.setAttribute('aria-describedby', described);
		else control.removeAttribute('aria-describedby');
	});
</script>

<div class={cn('flex w-full flex-col gap-1.5', className)} data-testid={`field-${id}`}>
	<div class="flex items-center justify-between gap-2">
		<label for={id} class="text-label text-foreground">{label}</label>
		{#if optional}
			<span class="text-helper text-muted-foreground">Optional</span>
		{:else if required}
			<span class="text-helper text-muted-foreground">Required</span>
		{/if}
	</div>
	{@render children()}
	{#if error}
		<p
			id={`${id}-error`}
			role="alert"
			class="flex items-center gap-1.5 text-helper text-failed-foreground"
		>
			<CircleAlert class="size-3.5 shrink-0" />
			{error}
		</p>
	{:else if helper}
		<p id={`${id}-helper`} class="text-helper text-muted-foreground">{helper}</p>
	{/if}
</div>
