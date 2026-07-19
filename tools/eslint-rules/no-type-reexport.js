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
    const filename = context.getFilename().replace(/\\/g, "/");
    const isSharedTypeEntry = filename.includes("/packages/shared/src/types/");

    return {
      ExportNamedDeclaration(node) {
        if (!node.source || isSharedTypeEntry) return;

        const hasTypeExport =
          node.exportKind === "type" ||
          node.specifiers.some((specifier) => specifier.exportKind === "type");
        if (!hasTypeExport) return;

        const sourceValue = node.source.value;
        if (!sourceValue.startsWith("@mcp_link/shared")) {
          context.report({
            node,
            messageId: "noTypeReexport",
          });
        }
      },
    };
  },
};
