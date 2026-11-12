// The web lint gate: the Rules of Hooks and complete effect dependencies are
// errors. An effect that deliberately runs on fewer dependencies says why on
// the line above its `eslint-disable-next-line`; one that only calls a
// callback takes it through `useStableCallback` instead. The shared layer may
// not import features.
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/", "node_modules/", "playwright-report/", "test-results/"],
  },
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
    },
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
  {
    // The shared layer sits below features: features import it, never the
    // other way round. Tests may reach across to exercise a feature.
    files: [
      "src/api/**/*.{ts,tsx}",
      "src/components/**/*.{ts,tsx}",
      "src/hooks/**/*.{ts,tsx}",
      "src/lib/**/*.{ts,tsx}",
      "src/observability/**/*.{ts,tsx}",
    ],
    ignores: ["**/__tests__/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["**/features", "**/features/**"],
              message:
                "The shared layer (api, components, hooks, lib, observability) can't import features/. Move what it needs down into the shared layer instead.",
            },
          ],
        },
      ],
    },
  },
);
