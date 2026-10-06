import { compile, compileModule } from 'svelte/compiler';
import { fileURLToPath } from 'node:url';

const projectSource = fileURLToPath(new URL('../../', import.meta.url));

Bun.plugin({
    name: 'skein-server-tests',
    setup(build) {
        build.onLoad({ filter: /\.svelte(?:\.ts)?$/ }, async ({ path }) => {
            if (!path.startsWith(projectSource)) return;
            const source = await Bun.file(path).text();
            const options = { filename: path, generate: 'server' };
            const result = path.endsWith('.svelte.ts')
                ? compileModule(new Bun.Transpiler({ loader: 'ts' }).transformSync(source), options)
                : compile(source, options);
            return { contents: result.js.code, loader: 'js' };
        });
    },
});
