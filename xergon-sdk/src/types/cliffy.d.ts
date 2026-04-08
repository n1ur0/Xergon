// Stub type declarations for @cliffy packages (Deno cliffy)
// These provide type-compatible stubs for CLI commands that reference them.

declare module '@cliffy/command' {
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  export interface Command {}
}

declare module '@cliffy/table' {
  export class Table {
    header(cols: string[]): this;
    rows(rows: unknown[][]): this;
    border(enabled: boolean): this;
    render(): void;
    toString(): string;
  }
}

declare module '@cliffy/colors' {
  const colors: {
    reset: (s: string) => string;
    bold: (s: string) => string;
    dim: (s: string) => string;
    italic: (s: string) => string;
    underline: (s: string) => string;
    strikethrough: (s: string) => string;
    red: (s: string) => string;
    green: (s: string) => string;
    yellow: (s: string) => string;
    blue: (s: string) => string;
    magenta: (s: string) => string;
    cyan: (s: string) => string;
    white: (s: string) => string;
    gray: (s: string) => string;
    bgRed: (s: string) => string;
    bgGreen: (s: string) => string;
    bgYellow: (s: string) => string;
    bgBlue: (s: string) => string;
    bgMagenta: (s: string) => string;
    bgCyan: (s: string) => string;
    bgWhite: (s: string) => string;
  };
  export { colors };
}
