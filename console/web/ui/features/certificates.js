export function createCertificateEditor({api, getGateway, isActive, showView, confirmAction, load, notice}) {
  const $ = id => document.getElementById(id);
  let certificateTarget = null;
  let certificateRequest = null;

  function renderCertificateStatus(status){$('certificate-status').textContent=status.configured?(status.valid?'当前配置证书校验通过':'当前配置证书校验失败：'+status.error):'尚未配置证书。请选择证书来源。';$('certificate-details').textContent=status.details||'暂无证书信息';}
  async function openCertificates(id){if(!isActive(id)){showView('gateways/'+encodeURIComponent(id)+'/certificates');return;}await loadCertificates(id);}
  async function loadCertificates(id){
   const r=getGateway(id);if(!r)return;const target={id,revision:r.revision};certificateTarget=target;certificateRequest=null;
   $('certificate-form').reset();$('csr-form').reset();$('certificate-mode').value='letsencrypt';certificateMode();$('certificate-error').textContent='';$('csr-error').textContent='';
   $('certificate-domain').textContent='申请域名 / IP：'+r.data.config.domain;
   $('certificate-status').textContent='正在读取证书状态…';$('certificate-details').textContent='';$('csr-status').textContent='正在读取申请…';
   $('csr-download').disabled=true;$('certificate-editor').hidden=false;
   try{const [status,request]=await Promise.all([api(`/gateways/${id}/certificate`),api(`/gateways/${id}/csr`)]);
   if(certificateTarget!==target)return;
   renderCertificateStatus(status);
   certificateRequest=request;renderCsr();
   }catch(error){if(certificateTarget!==target)return;$('certificate-status').textContent=error.message;$('csr-status').textContent='申请状态读取失败，请返回后重试';}
   certificateMode();
  }
  function renderCsr(){
   const ready=Boolean(certificateRequest?.csr);$('csr-download').disabled=!ready;
   $('csr-status').textContent=ready?'已有申请：'+certificateRequest.domain+'。下载 CSR 交给签发方，再上传签发结果。重新生成会替换当前申请。':'尚未生成 CSR。私钥保留在服务器，不通过页面返回。';
  }
  function certificateMode(){
   const mode=$('certificate-mode').value;
   const publicCa=mode==='letsencrypt', useCsr=mode==='csr';
   $('public-ca-guide').hidden=!publicCa;
   $('csr-section').hidden=!useCsr;
   $('certificate-upload-title').textContent=publicCa?'导入 Let’s Encrypt 证书':useCsr?'上传 CA 签发结果':'上传已有证书与私钥';
   const form=$('certificate-form');
   for(const [name,visible] of [['private_key',!useCsr],['ca',!publicCa]]){
    const input=form.elements[name];
    input.required=visible;input.disabled=!visible;input.parentElement.hidden=!visible;
   }
  }
  $('certificate-mode').onchange=certificateMode;
  $('certificate-x').onclick=$('certificate-close').onclick=()=>{
   if(certificateTarget)showView('gateways/'+encodeURIComponent(certificateTarget.id));
  };
  $('csr-form').onsubmit=async e=>{
   e.preventDefault();const b=e.target.querySelector('button[type=submit]');b.disabled=true;$('csr-error').textContent='';
   try{if(certificateRequest?.csr&&!await confirmAction({title:'重新生成 CSR',description:'当前待签发申请将被替换。已下载的旧 CSR 签发结果将无法再通过此申请上传；当前网关证书继续使用。',accept:'重新生成'}))return;
   const target=certificateTarget;if(!target)return;await api(`/gateways/${target.id}/csr`,{method:'POST',body:JSON.stringify({revision:target.revision,organization:e.target.elements.organization.value.trim(),ips:e.target.elements.ips.value.split(/[,，\s]+/).filter(Boolean)})});
   const current=await api(`/gateways/${target.id}/csr`);
   if(certificateTarget===target){certificateRequest=current;renderCsr();}
   }catch(error){$('csr-error').textContent=error.message;}finally{b.disabled=false;}
  };
  $('csr-download').onclick=()=>{if(!certificateRequest?.csr)return;const url=URL.createObjectURL(new Blob([certificateRequest.csr],{type:'application/pkcs10'}));const a=document.createElement('a');a.href=url;a.download=certificateRequest.domain+'.csr';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
  $('certificate-form').onsubmit=async e=>{
   e.preventDefault();const target=certificateTarget;const submit=e.target.querySelector('button[type=submit]');submit.disabled=true;$('certificate-error').textContent='';
   try{
   const mode=$('certificate-mode').value;
   const data={revision:target.revision};
   const fields=['certificate','intermediates'];
   if(mode==='letsencrypt')data.use_lets_encrypt=true;
   else fields.push('ca');
   if(mode!=='csr')fields.push('private_key');
   for(const field of fields){const file=e.target.elements[field].files[0];if(!file&&field==='intermediates')continue;if(!file||file.size>65536)throw Error('每个文件必须非空且不超过 64 KiB');data[field]=await file.text();}
   if(data.intermediates){data.certificate+='\n'+data.intermediates;delete data.intermediates;}if(data.certificate.length>65536)throw Error('合并后的证书链不能超过 64 KiB');
   if(mode==='csr'){if(!certificateRequest?.request_id)throw Error('请先生成 CSR 或选择导入已有私钥');data.request_id=certificateRequest.request_id;}
   const result=await api(`/gateways/${target.id}/certificate`,{method:'POST',body:JSON.stringify(data)});target.revision=result.record.revision;
   await load();if(certificateTarget!==target)return;e.target.reset();renderCsr();const status=await api(`/gateways/${target.id}/certificate`);if(certificateTarget!==target)return;renderCertificateStatus(status);notice('证书已校验并保存。返回网关详情，按提示让新证书生效。');
   }catch(error){$('certificate-error').textContent=error.message;}finally{submit.disabled=false;}
  };

  return {
    open: openCertificates,
    enter(id) {
      if (certificateTarget?.id !== id) return loadCertificates(id);
    },
    leave() {
      certificateTarget = null;
      certificateRequest = null;
    },
  };
}
