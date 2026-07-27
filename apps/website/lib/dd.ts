import dedent from "dedent";

/**
 * Dedent helper for authoring multi-line content strings — markdown/MDX copy or
 * code samples — as clean template literals. Lets every string in the content
 * modules be written inline and multi-line, with no `+` concatenation and no
 * `.join("\n")`.
 *
 * `alignValues` keeps interpolated multi-line values aligned to the indentation
 * of the line they sit on, so a nested snippet spliced into a larger block stays
 * readable rather than collapsing to the left margin.
 *
 * @example
 * const body = dd`
 *   Write one <code>.forge</code> schema. ForgeDB compiles it into a tailored
 *   Rust database, a TypeScript SDK, a REST API, and an OpenAPI spec.
 * `;
 */
export const dd = dedent.withOptions({ alignValues: true });

export default dd;
