import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    // Generated bindings are owned by the Rust exporters; build output and vendored deps are not ours.
    ignores: ["dist", "node_modules", "src-tauri", "playwright-report", "test-results"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: { ...globals.browser },
    },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      // The compiler-backed rules in the v7 preset flag patterns this UI relies on (effects that
      // mirror kernel state); the two classic rules are the ones that catch real bugs.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrors: "none" },
      ],
      "@typescript-eslint/consistent-type-imports": [
        "error",
        // `vi.mock` factories need `importOriginal<typeof import("...")>()`, which the rule's
        // annotation ban would reject.
        { prefer: "type-imports", fixStyle: "inline-type-imports", disallowTypeAnnotations: false },
      ],
      "no-console": ["warn", { allow: ["warn", "error"] }],
    },
  },
  {
    // Config files and Playwright specs run in Node, not the browser.
    files: ["*.config.ts", "tests/**/*.ts"],
    languageOptions: { globals: { ...globals.node } },
    rules: { "no-console": "off" },
  },
  {
    files: ["src/**/*.test.{ts,tsx}"],
    languageOptions: { globals: { ...globals.node } },
    rules: { "react-refresh/only-export-components": "off" },
  },
);
