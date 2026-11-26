import { clsx, type ClassValue } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';

/**
 * Semantic type utilities from routes/layout.css (`text-body`, `text-button`, ...). Without this,
 * tailwind-merge reads them as text *colours* and silently drops a real colour class
 * (e.g. `text-primary-foreground`) that appears before them.
 */
export const SEMANTIC_TEXT_SIZES = [
	'amount-xl',
	'amount-lg',
	'stat',
	'result-title',
	'page-title',
	'dialog-title',
	'card-title',
	'body',
	'label',
	'helper',
	'micro',
	'mono-id',
	'mono-va',
	'cell',
	'tab',
	'button'
];

export const twMergeConfig = {
	extend: { classGroups: { 'font-size': [{ text: SEMANTIC_TEXT_SIZES }] } }
} as const;

const twMerge = extendTailwindMerge(twMergeConfig);

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export type WithoutChild<T> = T extends { child?: unknown } ? Omit<T, 'child'> : T;
export type WithoutChildren<T> = T extends { children?: unknown } ? Omit<T, 'children'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
