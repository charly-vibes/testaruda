// Source module: user service
export interface User {
  id: number;
  name: string;
  email: string;
}

export function createUser(name: string, email: string): User {
  return { id: Date.now(), name, email };
}

export function getUserDisplayName(user: User): string {
  return `${user.name} <${user.email}>`;
}