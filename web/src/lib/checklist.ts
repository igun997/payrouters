import type { Component } from 'svelte';

/** One step of the first-run checklist on an empty Products page. */
export type ChecklistStep = {
	/** Icon shown inside the action button. */
	icon?: Component<{ class?: string }>;
	title: string;
	helper: string;
	status: 'done' | 'next' | 'todo';
	actionLabel: string;
	/** Set for a link; otherwise `onAction` runs. */
	href?: string;
	onAction?: () => void;
};
