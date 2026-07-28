// Source module: greeting utilities
export function greet(name: string): string {
  return `Hello, ${name}!`;
}

export function farewell(name: string): string {
  return `Goodbye, ${name}!`;
}

export function formatMessage(template: string, ...args: string[]): string {
  return template.replace(/{(\d+)}/g, (_, index) => args[parseInt(index)] ?? '');
}