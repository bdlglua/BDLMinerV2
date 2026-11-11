// Exercise the real WASM flasher against an ESP ROM/stub wire simulator.
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {chromium}=require('playwright');
function image(chip){const bytes=Buffer.from(Array.from({length:12345},(_,i)=>(i*37+11)%256));const offset=['esp32','esp32s2'].includes(chip)?0x1000:0;bytes[offset]=0xe9;bytes[offset+12]={esp32:0,esp32s2:2,esp32s3:9,esp32c3:5}[chip];bytes[offset+13]=0;return bytes;}
(async()=>{
 const child=spawn('python3',[path.join(__dirname,'site_fixture.py')],{env:{...process.env,BDL_FLASH_FIXTURE:'1'},stdio:['ignore','pipe','pipe']});let stderr='';child.stderr.on('data',d=>stderr+=d);
 const url=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error(stderr||'Fixture startup timeout')),30000);child.stdout.once('data',d=>{clearTimeout(timer);resolve(d.toString().trim())});child.once('exit',code=>{clearTimeout(timer);reject(Error('Fixture exited '+code+stderr))})});
 let browser;
 try{
  browser=await chromium.launch({headless:true,executablePath:process.env.BDL_CHROMIUM_PATH||(fs.existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined),args:['--no-sandbox']});
  for(const [board,chip,magic,usb,failure]of[['m5stickc-plus2','esp32',0x00f01d83,false],['esp32s2-headless','esp32s2',1990,false],['m5stack-s3','esp32s3',9,true],['esp32c3-headless','esp32c3',0x6921506f,true],['m5stickc-plus2','esp32',9,false],['m5stickc-plus2','esp32',0x00f01d83,false,'sha'],['m5stickc-plus2','esp32',0x00f01d83,false,'md5']]){
   const context=await browser.newContext({locale:'en-US'}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));const expected=image(chip),md5=[...crypto.createHash('md5').update(expected).digest()];if(failure==='md5')md5[0]^=1;
   if(failure==='sha')await page.route('**/manifest-m5stickc-plus2.json',async route=>{const response=await route.fetch(),data=await response.json();data.builds[0].parts[0].sha256='0'.repeat(64);await route.fulfill({response,json:data})});
   await page.addInitScript(({magic,md5,expected,usb})=>{
    const state=window.espFixture={operations:[],signals:[],flash:[],closed:false,stub:false,attached:false,errors:[]};let controller;
    const words=bytes=>new DataView(Uint8Array.from(bytes).buffer);
    function frame(bytes){const out=[0xc0];for(const b of bytes)out.push(...(b===0xc0?[0xdb,0xdc]:b===0xdb?[0xdb,0xdd]:[b]));out.push(0xc0);return Uint8Array.from(out)}
    function reply(op,data=[0,0],value=0){const bytes=new Uint8Array(8+data.length);bytes[0]=1;bytes[1]=op;const view=new DataView(bytes.buffer);view.setUint16(2,data.length,true);view.setUint32(4,value,true);bytes.set(data,8);const raw=frame(bytes);controller.enqueue(raw.slice(0,3));controller.enqueue(raw.slice(3));}
    function receive(raw){try{const packet=[];let escape=false;for(const b of raw){if(b===0xc0)continue;if(escape){packet.push(b===0xdc?0xc0:b===0xdd?0xdb:(()=>{throw Error('Invalid SLIP escape')})());escape=false;}else if(b===0xdb)escape=true;else packet.push(b);}const op=packet[1],view=words(packet),data=packet.slice(8);if(view.getUint16(2,true)!==data.length)throw Error('Packet length mismatch');state.operations.push(op);let checksum=0xef;
      if(op===7||op===3){for(const b of data.slice(16))checksum^=b;if(view.getUint32(4,true)!==checksum)throw Error('Packet checksum mismatch');}
      if(op===8)reply(op,[0,0,0,0],0x20120707);
      else if(op===10)reply(op,[0,0,0,0],magic);
      else if(op===5||op===7)reply(op,[0,0,0,0]);
      else if(op===6){reply(op,[0,0,0,0]);state.stub=true;controller.enqueue(frame([79,72,65,73]));}
      else if(op===13){if(!state.stub)throw Error('SPI attach before stub');state.attached=true;reply(op);}
      else if(op===0xd0){if(!state.attached)throw Error('Erase before SPI initialization');state.flash=[];reply(op);}
      else if(op===2){const values=words(data);if(values.getUint32(0,true)!==expected.length||values.getUint32(8,true)!==4096)throw Error('Invalid flash geometry');reply(op);}
      else if(op===3){const values=words(data),index=values.getUint32(4,true);if(values.getUint32(0,true)!==4096)throw Error('Invalid flash block');for(const [i,b]of data.slice(16).entries())state.flash[index*4096+i]=b;reply(op);}
      else if(op===19){const values=words(data);if(values.getUint32(4,true)!==expected.length)throw Error('Invalid verification range');if(!expected.every((b,i)=>state.flash[i]===b))throw Error('Written firmware differs');reply(op,[...md5,0,0]);}
      else if(op===4)reply(op);else throw Error('Unexpected command '+op);
    }catch(error){state.errors.push(error.message);controller.error(error);throw error;}}
    const port={getInfo:()=>usb?{usbVendorId:0x303a,usbProductId:0x1001}:{usbVendorId:0x10c4,usbProductId:0xea60},async open(){this.readable=new ReadableStream({start(c){controller=c}});this.writable=new WritableStream({write:receive})},async setSignals(signals){state.signals.push(signals)},async close(){state.closed=true}};
    Object.defineProperty(navigator,'serial',{configurable:true,value:{async requestPort(){return port}}});
   },{magic,md5,expected:[...expected],usb});
   await page.goto(url+'/flash.html?board='+board);await page.waitForFunction(()=>document.body.dataset.runtime==='ready');await page.locator('#installer [slot=activate]').click();await page.locator('#flash-connect').click();
   const mismatch=chip==='esp32'&&magic===9;
   await page.locator('#flash-status').filter({hasText:mismatch?/Chip mismatch/:failure==='sha'?/checksum does not match/:failure==='md5'?/Flash verification failed/:/Installation verified/}).waitFor({timeout:20000});await page.waitForFunction(()=>window.espFixture.closed);
   const state=await page.evaluate(()=>window.espFixture);assert.deepEqual(errors,[]);assert.deepEqual(state.errors,[]);assert.equal(state.operations.includes(0xd0),!mismatch&&failure!=='sha','chip identity must be checked before erase');assert.equal(state.operations.includes(3),!mismatch&&failure!=='sha');if(!mismatch&&failure!=='sha'){assert.deepEqual(state.flash.slice(0,expected.length),[...expected]);assert.ok(state.operations.indexOf(19)>state.operations.lastIndexOf(3),'readback verification follows writing');assert.equal(state.operations.includes(4),failure!=='md5','failed verification must not boot the image');}
   await context.close();console.log('PASS Rust Web Serial:',board,usb?'USB JTAG':'UART',mismatch?'rejects mismatched chip':failure?'rejects '+failure+' mismatch':'writes and verifies exact image');
  }
 }finally{if(browser)await browser.close();child.kill();}
})().catch(error=>{console.error(error);process.exitCode=1});
