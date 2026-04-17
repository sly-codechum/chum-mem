import * as esbuild from 'esbuild';

await esbuild.build({
  entryPoints: ['src/graph/adapters/client.ts'],
  bundle: true,
  minify: process.argv.includes('--minify'),
  sourcemap: true,
  format: 'esm',
  target: 'es2022',
  outfile: 'dist/public/graph-client.js',
  external: [],
});

console.log('Client bundle built → dist/public/graph-client.js');
