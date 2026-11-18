// Query the generated Rustdoc index with Rustdoc's own search implementation.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

async function search(root) {
    const assets = path.join(root, "static.files");
    const asset = (name) => path.join(assets, fs.readdirSync(assets).find(
        (entry) => entry.startsWith(name + "-") && entry.endsWith(".js"),
    ));
    // The browser loads these helpers from Rustdoc's storage script.
    const storage = fs.readFileSync(asset("storage"), "utf8");
    for (const name of ["nonnull", "nonundef"]) {
        const start = storage.indexOf("function " + name + "(");
        const end = storage.indexOf("function ", start + 9);
        globalThis[name] = vm.runInThisContext("(" + storage.slice(start, end) + ")");
    }
    const { Stringdex, RoaringBitmap } = require(asset("stringdex"));
    const { initSearch } = require(asset("search"));
    const index = path.join(root, "search.index");
    let callbacks;
    const load = (...parts) => vm.runInNewContext(
        fs.readFileSync(path.join(index, ...parts), "utf8"), callbacks,
    );
    const { docSearch, DocSearch } = await initSearch(Stringdex, RoaringBitmap, {
        loadRoot: (handlers) => { callbacks = handlers; load("root.js"); },
        loadTreeByHash: (hash) => load(hash + ".js"),
        loadDataByNameAndHash: (name, hash) => load(name, hash + ".js"),
    });
    const { others } = await docSearch.execQuery(DocSearch.parseQuery("Top::count"), "call_docs_api", "call_docs_api");
    const matches = [];
    for await (const item of others) {
        matches.push({ name: item.item?.name ?? item.name, href: item.href });
    }
    console.log(JSON.stringify(matches));
    if (!matches.some((item) => item.href.endsWith("/champion_mastery/enum.Top.html#method.count"))) {
        throw new Error("Rustdoc search for Top::count did not find the setter on the endpoint type");
    }
    if (matches.some((item) => item.href.includes("/struct.Call.html#method.count"))) {
        throw new Error("Rustdoc search exposes a setter specific to one endpoint on the shared Call page");
    }
}

search(path.resolve(process.argv[2])).catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
