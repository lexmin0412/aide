// ESLint 9 flat config for the aide frontend.
//
// Prettier is intentionally not part of this setup: the repository has no
// formatting baseline yet and adding it would trigger a repo-wide reformat.
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import i18next from "eslint-plugin-i18next";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "coverage/**",
      // Local pnpm store used by sandboxed environments (see .gitignore).
      ".pnpm-store/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  jsxA11y.flatConfigs.recommended,
  {
    plugins: { i18next },
    rules: {
      // TODO: enable once user-facing strings are extracted to i18next. The
      // app still hard-codes many UI strings, so this would flood the report.
      "i18next/no-literal-string": "off",
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      // Hooks correctness is a blocking error.
      "react-hooks/rules-of-hooks": "error",

      // The rules below cannot pass on the current codebase yet. They are kept
      // as warnings so CI stays green while the violations are burned down;
      // promote each one back to "error" once its count hits zero.
      // (react-hooks v7 added the compiler-oriented checks.)
      "react-hooks/exhaustive-deps": "warn",
      "react-hooks/refs": "warn",
      "react-hooks/set-state-in-effect": "warn",
      "react-hooks/preserve-manual-memoization": "warn",
      // Intentional empty catch/finally blocks exist in the IPC/probing paths.
      "no-empty": "warn",
      // `any` is still used in a few Tauri IPC boundaries.
      "@typescript-eslint/no-explicit-any": "warn",
      // jsx-a11y: recommended stays on, but these rules fire on existing
      // clickable-div / autofocus patterns and are downgraded until the
      // components are reworked.
      "jsx-a11y/no-static-element-interactions": "warn",
      "jsx-a11y/click-events-have-key-events": "warn",
      "jsx-a11y/no-autofocus": "warn",
      "jsx-a11y/no-noninteractive-tabindex": "warn",
    },
  },
);
