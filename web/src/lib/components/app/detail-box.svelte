<script lang="ts">
	/** Key/value summary inside a dialog (what is being revoked, deleted or blocked). */
	let {
		rows
	}: {
		rows: { label: string; value: string; mono?: boolean }[];
	} = $props();
</script>

<dl class="flex flex-col divide-y rounded-md border bg-card" data-testid="detail-box">
	{#each rows as row (row.label)}
		<div
			class="flex flex-col gap-1 px-3.5 py-2.5 sm:flex-row sm:items-center sm:justify-between sm:gap-4"
		>
			<dt class="text-helper text-muted-foreground">{row.label}</dt>
			<dd
				class="min-w-0 text-body break-words text-foreground sm:max-w-[60%] sm:text-right {row.mono
					? 'text-mono-id font-mono break-all'
					: ''}"
			>
				{row.value}
			</dd>
		</div>
	{/each}
</dl>
