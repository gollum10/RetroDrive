import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import useImagePreviews from '../../src/theme-studio/use-image-previews';
import SceneCanvas from '../../src/theme-studio/SceneCanvas';
import { newDocument } from '../../src/theme-studio/document';
import { attachImage } from '../../src/theme-studio/image-assets';
import '../../src/theme-studio/theme-studio.css';

const canvas=document.createElement('canvas');canvas.width=1;canvas.height=1;canvas.getContext('2d')!.fillRect(0,0,1,1);
const url=canvas.toDataURL('image/png');
const initial=newDocument({name:'Saved image fixture',author:'',description:'',version:'1.0.0',license:'original',target:'',targetVersion:'',display:'16:9',template:'blank'});
const descriptor={sha256:'a'.repeat(64),path:`assets/images/${'a'.repeat(64)}.png`,bytes:100,width:1,height:1};
const first=attachImage(initial,initial.scenes[0].id,descriptor,'original','');
const saved=attachImage(first.project,initial.scenes[0].id,descriptor,'original','').project;
const service={busy:false,forcedBusy:0,permanent:false,delay:80,calls:[] as string[]};
Object.assign(window,{previewService:service,__TAURI_INTERNALS__:{invoke:async(command:string,args:{projectId:string})=>{
    if(command!=='preview_theme_png')throw new Error('Unexpected fixture command');
    service.calls.push(args.projectId);
    if(service.permanent)throw {code:'image_missing',message:'Fixture image unavailable'};
    if(service.busy||service.forcedBusy>0){service.forcedBusy=Math.max(0,service.forcedBusy-1);throw {code:'image_busy',message:'Fixture worker busy'};}
    service.busy=true;
    try{await new Promise(resolve=>setTimeout(resolve,service.delay));return url;}finally{service.busy=false;}
}}});
function Preview({projectId,empty}:{projectId:string;empty:boolean}){
    const previews=useImagePreviews(projectId,empty?[]:saved.assets,0);
    Object.assign(window,{previewRetry:previews.retry});
    return <><output data-error>{previews.error}</output><SceneCanvas scene={saved.scenes[0]} selected={null} disabled={false} previewImages={previews.images} onSelect={()=>{}} onMove={()=>{}} onResize={()=>{}} onEndGroup={()=>{}}/></>;
}
function Harness(){
    const [projectId,setProject]=useState('saved-one');const [generation,setGeneration]=useState(0);const [empty,setEmpty]=useState(false);
    Object.assign(window,{previewHarness:{setProject,setEmpty,remount:()=>setGeneration(value=>value+1)}});
    return <div className="theme-studio"><Preview key={generation} projectId={projectId} empty={empty}/></div>;
}
createRoot(document.getElementById('root')!).render(<React.StrictMode><Harness/></React.StrictMode>);
