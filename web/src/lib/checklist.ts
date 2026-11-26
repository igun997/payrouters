/** One step of the first-run checklist on an empty Products page. */
export type ChecklistStep = {
	title: string;
	helper: string;
	status: 'done' | 'next' | 'todo';
	actionLabel: string;
	/** Set for a link; otherwise `onAction` runs. */
	href?: string;
	onAction?: () => void;
};
