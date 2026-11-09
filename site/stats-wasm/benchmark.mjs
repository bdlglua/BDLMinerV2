import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

const count=2016;
const points=Array.from({length:count},(_,i)=>({at:i*300000,value:1e12+i%1000}));
const start=points[0].at,end=points.at(-1).at,left=86,right=948,bottom=286,top=16,minimum=1e12,maximum=1e12+1000;
const wasmPath=fileURLToPath(new URL('./target/wasm32-unknown-unknown/release/bdlminer_stats_wasm.wasm',import.meta.url));
const {instance}=await WebAssembly.instantiate(readFileSync(wasmPath),{});
const {exports}=instance;
const pointer=exports.stats_buffer_ptr();
const buffer=new Float64Array(exports.memory.buffer,pointer,count*2);

function projectJavaScript(){
 for(const point of points){
  point.x=left+(right-left)*(point.at-start)/(end-start);
  point.y=bottom-(bottom-top)*(point.value-minimum)/(maximum-minimum);
 }
 return points.at(-1).x;
}

function projectRust(){
 for(let i=0;i<count;i++){
  buffer[i*2]=points[i].at;
  buffer[i*2+1]=points[i].value;
 }
 if(exports.stats_project_points(pointer,count,start,end,left,right,bottom,top,minimum,maximum)!==count)
  throw new Error('Rust chart projection rejected the benchmark points');
 for(let i=0;i<count;i++){
  points[i].x=buffer[i*2];
  points[i].y=buffer[i*2+1];
 }
 return points.at(-1).x;
}

for(let i=0;i<100;i++){projectJavaScript();projectRust()}
for(let i=0;i<count;i++){
 const x=left+(right-left)*(points[i].at-start)/(end-start);
 const y=bottom-(bottom-top)*(points[i].value-minimum)/(maximum-minimum);
 assert.ok(Math.abs(points[i].x-x)<1e-9&&Math.abs(points[i].y-y)<1e-9,'Rust and JavaScript coordinates must match');
}

function measure(project){
 const samples=[];
 for(let run=0;run<7;run++){
  const started=performance.now();
  for(let i=0;i<300;i++)project();
  samples.push(performance.now()-started);
 }
 samples.sort((a,b)=>a-b);
 return samples[3]/300;
}

const javascriptMs=measure(projectJavaScript),rustMs=measure(projectRust);
console.log(`Chart projection (${count} points; median of 7 x 300 runs): JS ${javascriptMs.toFixed(4)} ms, Rust/WASM ${rustMs.toFixed(4)} ms (${(rustMs/javascriptMs).toFixed(2)}x JS time)`);
