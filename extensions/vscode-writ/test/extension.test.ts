import { describe, it, expect } from "vitest";
import * as fs from "fs";
import * as path from "path";

const root = path.resolve(__dirname, "..");

describe("package.json", () => {
  const pkg = JSON.parse(
    fs.readFileSync(path.join(root, "package.json"), "utf-8")
  );

  it("registers the writ language", () => {
    const langs = pkg.contributes.languages;
    expect(langs).toHaveLength(1);
    expect(langs[0].id).toBe("writ");
    expect(langs[0].extensions).toContain(".writ");
  });

  it("registers the TextMate grammar", () => {
    const grammars = pkg.contributes.grammars;
    expect(grammars).toHaveLength(1);
    expect(grammars[0].language).toBe("writ");
    expect(grammars[0].scopeName).toBe("source.writ");
  });

  it("registers the writ debugger", () => {
    const debuggers = pkg.contributes.debuggers;
    expect(debuggers).toHaveLength(1);
    expect(debuggers[0].type).toBe("writ");
    expect(debuggers[0].configurationAttributes.launch.properties.port.default).toBe(7778);
  });

  it("has correct configuration defaults", () => {
    const props = pkg.contributes.configuration.properties;
    expect(props["writ.lspPath"].default).toBe("writ-lsp");
    expect(props["writ.hotReload.enabled"].default).toBe(true);
    expect(props["writ.hotReload.mechanism"].default).toBe("socket");
    expect(props["writ.hotReload.mechanism"].enum).toEqual(["socket", "pipe", "file"]);
    expect(props["writ.hotReload.address"].default).toBe("127.0.0.1:7777");
  });
});

describe("TextMate grammar", () => {
  const grammar = JSON.parse(
    fs.readFileSync(
      path.join(root, "syntaxes", "writ.tmLanguage.json"),
      "utf-8"
    )
  );

  it("has valid JSON structure", () => {
    expect(grammar.name).toBe("Writ");
    expect(grammar.scopeName).toBe("source.writ");
    expect(grammar.patterns).toBeDefined();
    expect(grammar.repository).toBeDefined();
  });

  it("includes all expected pattern groups", () => {
    const patternNames = grammar.patterns.map(
      (p: { include: string }) => p.include
    );
    expect(patternNames).toContain("#comments");
    expect(patternNames).toContain("#strings");
    expect(patternNames).toContain("#keywords");
    expect(patternNames).toContain("#operators");
    expect(patternNames).toContain("#numeric-literals");
  });

  it("covers all control keywords", () => {
    const keywords = grammar.repository.keywords.patterns[0].match;
    for (const kw of ["if", "else", "when", "while", "for", "in", "break", "continue", "return"]) {
      expect(keywords).toContain(kw);
    }
  });

  it("covers all storage keywords", () => {
    const storage = grammar.repository.storage.patterns[0].match;
    for (const kw of ["class", "func", "trait", "enum", "struct", "let", "var", "const"]) {
      expect(storage).toContain(kw);
    }
  });
});

describe("language configuration", () => {
  const langConfig = JSON.parse(
    fs.readFileSync(path.join(root, "language-configuration.json"), "utf-8")
  );

  it("defines comment rules", () => {
    expect(langConfig.comments.lineComment).toBe("//");
    expect(langConfig.comments.blockComment).toEqual(["/*", "*/"]);
  });

  it("defines bracket pairs", () => {
    expect(langConfig.brackets).toBeDefined();
    expect(langConfig.brackets.length).toBeGreaterThan(0);
  });
});

describe("test fixtures", () => {
  const fixturesDir = path.join(root, "test", "fixtures");

  it("all fixtures use .writ extension", () => {
    const files = fs.readdirSync(fixturesDir);
    const writFiles = files.filter((f) => f.endsWith(".writ"));
    expect(writFiles.length).toBeGreaterThan(0);
    expect(files.filter((f) => f.endsWith(".wrt"))).toHaveLength(0);
  });
});
