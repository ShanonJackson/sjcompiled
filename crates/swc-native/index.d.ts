// Mirror the @swc/core surface we actually use in the parity harness.
// Keep this list aligned with `parity-harness/babel-plugin/engines.ts`.

export interface SwcOutput {
  code: string;
  map?: string;
}

export interface SwcOptions {
  filename?: string;
  jsc?: {
    target?: string;
    parser?: { syntax?: string; tsx?: boolean };
    transform?: {
      verbatimModuleSyntax?: boolean;
      react?: { runtime?: "classic" | "automatic"; useSpread?: boolean };
    };
    preserveAllComments?: boolean;
    experimental?: {
      runPluginFirst?: boolean;
      plugins?: Array<[string, Record<string, unknown>]>;
    };
  };
  // Pass-through anything else; @swc/core has a much wider surface
  // and we forward unknown keys to it via JSON.
  [key: string]: unknown;
}

export declare const version: string;

export declare function transformSync(src: string, options?: SwcOptions): SwcOutput;
