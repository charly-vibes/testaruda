import { describe, it, expect } from 'vitest';
import { createUser, getUserDisplayName } from '../src/user';

describe('UserService', () => {
  it('creates a user with name and email', () => {
    const user = createUser('Alice', 'alice@example.com');
    expect(user.name).toBe('Alice');
    expect(user.email).toBe('alice@example.com');
  });

  it('formats user display name', () => {
    const user = createUser('Bob', 'bob@example.com');
    const display = getUserDisplayName(user);
    expect(display).toBe('Bob <bob@example.com>');
  });
});