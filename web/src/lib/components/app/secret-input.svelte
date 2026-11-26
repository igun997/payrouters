<script lang="ts">
	/**
	 * Write-only secret field. The server never returns a secret, only its mask (`••••d1e4`).
	 * `saved` is that mask: it shows as the placeholder, and a blank value keeps the stored secret
	 * (the caller omits empty fields). The eye toggles visibility of what the user types, not the mask.
	 */
	import { Input } from '#lib/components/ui/input';
	import { Button } from '#lib/components/ui/button';
	import Eye from '@lucide/svelte/icons/eye';
	import EyeOff from '@lucide/svelte/icons/eye-off';
	import { cn } from '#lib/utils';

	let {
		id,
		value = $bindable(''),
		saved = null,
		placeholder = '',
		invalid = false,
		describedBy,
		class: className
	}: {
		id: string;
		value?: string;
		/** Masked value the server holds, e.g. "••••d1e4". */
		saved?: string | null;
		/** Shown when nothing is saved yet. */
		placeholder?: string;
		invalid?: boolean;
		describedBy?: string;
		class?: string;
	} = $props();

	let visible = $state(false);
</script>

<div class={cn('relative', className)}>
	<Input
		{id}
		type={visible ? 'text' : 'password'}
		autocomplete="off"
		autocapitalize="off"
		spellcheck={false}
		bind:value
		placeholder={saved || placeholder}
		aria-invalid={invalid || undefined}
		aria-describedby={describedBy}
		class="pe-12 font-mono"
		data-testid="secret-input"
	/>
	<Button
		type="button"
		variant="ghost"
		size="icon-sm"
		class="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground"
		aria-label={visible ? 'Hide value' : 'Show value'}
		aria-pressed={visible}
		onclick={() => (visible = !visible)}
	>
		{#if visible}
			<EyeOff />
		{:else}
			<Eye />
		{/if}
	</Button>
</div>
