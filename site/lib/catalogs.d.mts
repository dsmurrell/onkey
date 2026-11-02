export const PSEUDO: string;
export function messagesDir(): string;
export function catalogLocales(pseudo?: boolean): string[];
export function flatten(messages: object, prefix?: string): Record<string, string>;
export function merge<T extends object>(base: T, over: object | undefined): T;
export function pseudoMessage(text: string): string;
export function pseudoMessages<T extends object>(messages: T): T;
export function placeholders(text: string): string;
export function checkCatalog(en: object, catalog: object): string[];
