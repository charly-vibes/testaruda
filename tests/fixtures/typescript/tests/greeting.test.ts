import { describe, it, expect } from 'vitest';
import { greet, farewell } from '../src/greeting';

describe('Greeting', () => {
  it('greets by name', () => {
    expect(greet('World')).toBe('Hello, World!');
  });

  it('says farewell', () => {
    expect(farewell('World')).toBe('Goodbye, World!');
  });
});

describe('Greeting with describe.each', () => {
  describe.each([
    ['morning', 'Good morning'],
    ['evening', 'Good evening'],
  ])('time of day: %s', (time: string, expected: string) => {
    it(`greets for ${time}`, () => {
      expect(greet(time)).toBe(`Hello, ${time}!`);
    });
  });
});