module.exports = {
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow re-exporting types from modules other than @mcp_link/shared",
      category: "Best Practices",
      recommended: true,
    },
    messages: {
      noTypeReexport:
        "Type re-exports are only allowed from @mcp_link/shared package. Import types directly from @mcp_link/shared instead.",
    },
    schema: [],
  },
  create(context) {
    return {
      ExportNamedDeclaration(node) {
        // Check if this is a type re-export
        if (node.exportKind === "type" && node.source) {
          const sourceValue = node.source.value;

          // Allow re-exports only from @mcp_link/shared
          if (!sourceValue.startsWith("@mcp_link/shared")) {
            context.report({
              node,
              messageId: "noTypeReexport",
            });
          }
        }

        // Check for export type { ... } from "..."
        if (node.specifiers && node.source) {
          const hasTypeExport = node.specifiers.some(
            (spec) => spec.exportKind === "type" || node.exportKind === "type",
          );

          if (hasTypeExport) {
            const sourceValue = node.source.value;
            if (!sourceValue.startsWith("@mcp_link/shared")) {
              context.report({
                node,
                messageId: "noTypeReexport",
              });
            }
          }
        }
      },

      // Check for export { type X } from "..."
      ExportSpecifier(node) {
        if (node.exportKind === "type") {
          const parent = node.parent;
          if (parent.type === "ExportNamedDeclaration" && parent.source) {
            const sourceValue = parent.source.value;
            if (!sourceValue.startsWith("@mcp_link/shared")) {
              context.report({
                node,
                messageId: "noTypeReexport",
              });
            }
          }
        }
      },
    };
  },
};
