// Windows-native, offline factory preparation. No Docker daemon or ARM execution.
package main

import (
 "context"
 "crypto/sha256"
 "encoding/binary"
 "encoding/hex"
 "encoding/json"
 "errors"
 "fmt"
 "io"
 "net/http"
 "os"
 "os/exec"
 "path/filepath"
 "sort"
 "strings"
 "time"
)

const repoURL = "https://github.com/David2766/K11C-HAOS"
const imageName = "ghcr.io/david2766/k11c-haos-connectivity"
const slug = "157e89e9_k11c_connectivity"
var toolsManifestHash string // Set by the source build; never accepts alternate tools.

type Profile struct {
 HAOS string `json:"haos"`
 Kernel string `json:"kernel"`
 OfficialHash string `json:"official_raw_sha256"`
 Version string `json:"app_version"`
 Digest string `json:"image_digest"`
 ImageID string `json:"image_id"`
 Commit string `json:"catalog_commit"`
 ConfigHash string `json:"config_sha256"`
 StateHash string `json:"state_sha256"`
 State json.RawMessage `json:"state"`
 Containerd string `json:"containerd"`
 Verified bool `json:"locally_verified"`
}
type Request struct { Raw string `json:"raw"`; Cache string `json:"cache"`; Profile Profile `json:"profile"` }
type Receipt struct { Bytes int64 `json:"bytes"`; SHA256 string `json:"sha256"`; Seconds float64 `json:"seconds"`; ExistingImages int `json:"existing_images"` }
type preparer struct { ctx context.Context; tools, work string; output string; stages map[string]float64 }

func hashBytes(b []byte) string { h:=sha256.Sum256(b); return hex.EncodeToString(h[:]) }
func hashFile(path string) (string,int64,error) { f,e:=os.Open(path); if e!=nil{return "",0,e}; defer f.Close(); h:=sha256.New(); n,e:=io.Copy(h,f); return hex.EncodeToString(h.Sum(nil)),n,e }
func hexID(s string,n int) bool { if len(s)!=n{return false}; for _,c:=range s { if !(c>='0'&&c<='9'||c>='a'&&c<='f'){return false} };return true }
func progress(phase string,n int64,total any) { _=json.NewEncoder(os.Stderr).Encode(map[string]any{"phase":phase,"completed":n,"total":total}) }
func failCheck(ok bool, why string) error { if !ok{return errors.New(why)}; return nil }
func validateProfile(p Profile) error {
 if !p.Verified || p.Containerd!="2.3.4" || !hexID(p.OfficialHash,64) || !hexID(p.Commit,40) || !hexID(p.ConfigHash,64) || !hexID(p.StateHash,64) || !strings.HasSuffix(p.Kernel,"-haos") || !hexID(strings.TrimPrefix(p.ImageID,"sha256:"),64) || !strings.HasPrefix(p.ImageID,"sha256:") || !strings.HasPrefix(p.Digest,imageName+"@sha256:") || !hexID(strings.TrimPrefix(p.Digest,imageName+"@sha256:"),64) { return errors.New("Unverified or unsupported compatibility profile") }
 for _,v:=range []string{p.HAOS,p.Version} { if len(v)>32||len(strings.Split(v,"."))<2 {return errors.New("Invalid version")};for _,c:=range v {if !(c=='.'||c>='0'&&c<='9'){return errors.New("Invalid version")}} }
 var state struct { System map[string]map[string]any `json:"system"`; User map[string]map[string]any `json:"user"` }
 if e:=json.Unmarshal(p.State,&state);e!=nil{return e}
 var value any; if e:=json.Unmarshal(p.State,&value);e!=nil{return e}; canonical,e:=json.Marshal(value);if e!=nil{return e}
 if hashBytes(canonical)!=p.StateHash{return errors.New("Supervisor state hash mismatch")}
 s,u:=state.System[slug],state.User[slug]
 options,_:=u["options"].(map[string]any)
 arch,_:=s["arch"].([]any)
 if len(state.System)!=1||len(state.User)!=1||len(u)!=5||s["version"]!=p.Version||u["version"]!=p.Version||s["image"]!=imageName||u["image"]!=imageName||s["slug"]!="k11c_connectivity"||s["repository"]!="157e89e9"||s["startup"]!="system"||len(arch)!=1||arch[0]!="aarch64"||u["boot"]!="auto"||u["protected"]!=false||len(options)!=1||options["enabled"]!=true {return errors.New("Invalid installed app/autostart state")}
 return nil
}

// Every executable and its loadable DLLs are fixed build inputs. Do not search PATH.
func verifyTools(dir string) error {
 b,e:=os.ReadFile(filepath.Join(dir,"tools.json"));if e!=nil{return e}
 if !hexID(toolsManifestHash,64)||hashBytes(b)!=toolsManifestHash{return errors.New("Native tool manifest changed")}
 var hashes map[string]string;if e=json.Unmarshal(b,&hashes);e!=nil{return e}
 for _,name:=range []string{"debugfs.exe","e2fsck.exe","resize2fs.exe","cygwin1.dll"} {if !hexID(hashes[name],64){return errors.New("Incomplete native tool set")}}
 for name,h:=range hashes {if filepath.Base(name)!=name||!hexID(h,64){return errors.New("Invalid native tool name")};actual,_,e:=hashFile(filepath.Join(dir,name));if e!=nil{return e};if actual!=h{return fmt.Errorf("Native tool changed: %s",name)}}
 // The signed build's directory is private to these pinned dependencies.
 entries,e:=os.ReadDir(dir);if e!=nil{return e};for _,entry:=range entries {n:=entry.Name(); if strings.HasSuffix(strings.ToLower(n),".dll")&&hashes[n]=="" {return errors.New("Unexpected native tool DLL")}}
 return nil
}
type bounded struct { data []byte }
func (b *bounded) Write(p []byte)(int,error){n:=len(p);if len(b.data)<4*1024*1024{left:=4*1024*1024-len(b.data);if len(p)>left{p=p[:left]};b.data=append(b.data,p...)};return n,nil}
func (p *preparer) tool(name string,args ...string)(string,error){
 ctx,cancel:=context.WithTimeout(p.ctx,3*time.Minute);defer cancel()
 cmd:=exec.CommandContext(ctx,filepath.Join(p.tools,name+".exe"),args...);cmd.Dir=p.work
 cmd.Env=append(os.Environ(),"LANG=C.UTF-8","LC_ALL=C.UTF-8")
 var output bounded;cmd.Stdout=&output;cmd.Stderr=&output;hideWindow(cmd)
 e:=cmd.Run();s:=string(output.data);if e!=nil{return s,fmt.Errorf("%s: %w: %s",name,e,s)};return s,nil
}
func quote(s string)(string,error){if strings.ContainsAny(s,"\"\r\n\x00"){return "",errors.New("Unsupported filename")};return "\""+filepath.ToSlash(s)+"\"",nil}
func (p *preparer) debug(command string)(string,error){return p.tool("debugfs","-R",command,p.output)}
func (p *preparer) dump(target,path string)error{q,e:=quote(path);if e!=nil{return e};_,e=p.debug("dump "+target+" "+q);if e!=nil{return e};_,e=os.Stat(path);return e}
func (p *preparer) checkFile(target,source,mode string)error{
 check:=filepath.Join(p.work,"readback");if e:=os.Remove(check);e!=nil&&!os.IsNotExist(e){return e};if e:=p.dump(target,check);e!=nil{return e}
 a,na,e:=hashFile(check);if e!=nil{return e};b,nb,e:=hashFile(source);if e!=nil{return e};if a!=b||na!=nb{return fmt.Errorf("Filesystem readback mismatch: %s",target)}
 return p.checkInode(target,mode)
}
func (p *preparer) checkInode(target,mode string)error{
 out,e:=p.debug("stat "+target);if e!=nil{return e}
 if !strings.Contains(out,"Mode:  "+mode)||!strings.Contains(out,"User:     0   Group:     0") {return fmt.Errorf("Filesystem ownership/mode mismatch: %s: %s",target,out)};return nil
}
func (p *preparer) inject(files map[string]string,dirs []string)error{
 sort.Strings(dirs);lines:=[]string{}
 for _,d:=range dirs { lines=append(lines,"mkdir "+d,"set_inode_field "+d+" uid 0","set_inode_field "+d+" gid 0","set_inode_field "+d+" mode 040755") }
 targets:=[]string{};for target:=range files{targets=append(targets,target)};sort.Strings(targets)
 lines=append(lines,"rm /docker/containerd/daemon/io.containerd.metadata.v1.bolt/meta.db")
 for _,t:=range targets {q,e:=quote(files[t]);if e!=nil{return e};mode:="0100644";if strings.HasPrefix(t,"/supervisor/"){mode="0100600";if strings.Contains(t,"/apps/git/"){mode="0100644"}}
  lines=append(lines,"write "+q+" "+t,"set_inode_field "+t+" uid 0","set_inode_field "+t+" gid 0","set_inode_field "+t+" mode "+mode)
 }
 batch:=filepath.Join(p.work,"write.txt");if e:=os.WriteFile(batch,[]byte(strings.Join(lines,"\n")+"\n"),0600);e!=nil{return e}
 out,e:=p.tool("debugfs","-w","-f",batch,p.output);if e!=nil{return e}
 // debugfs can exit zero after a failed batch command; actual file readback follows.
 for _,needle:=range []string{"not found","No such","File exists","Could not","while "} {if strings.Contains(out,needle){return errors.New("Filesystem command failed: "+out)}}
 // One inode audit and one recursive repo readback, rather than two processes
 // for every source file. Content hashes remain exhaustive.
 checks:=[]string{};for _,d:=range dirs{checks=append(checks,"stat "+d)};for _,t:=range targets{checks=append(checks,"stat "+t)}
 audit:=filepath.Join(p.work,"inodes.txt");if e=os.WriteFile(audit,[]byte(strings.Join(checks,"\n")+"\n"),0600);e!=nil{return e}
 stats,e:=p.tool("debugfs","-f",audit,p.output);if e!=nil{return e}
 if strings.Count(stats,"User:     0   Group:     0")!=len(checks)||strings.Count(stats,"Mode:  0755")!=len(dirs)||strings.Count(stats,"Mode:  0600")!=2||strings.Count(stats,"Mode:  0644")!=len(targets)-2 {return errors.New("Filesystem inode audit failed")}
 checkDir:=filepath.Join(p.work,"repo-readback");if e=os.Mkdir(checkDir,0700);e!=nil{return e};q,e:=quote(checkDir);if e!=nil{return e}
 if _,e=p.debug("rdump /supervisor/apps/git/157e89e9 "+q);e!=nil{return e}
 for _,t:=range targets {
  if strings.HasPrefix(t,"/supervisor/apps/git/157e89e9/") {file:=filepath.Join(checkDir,"157e89e9",filepath.FromSlash(strings.TrimPrefix(t,"/supervisor/apps/git/157e89e9/")));a,n,e:=hashFile(file);if e!=nil{return e};b,m,e:=hashFile(files[t]);if e!=nil{return e};if a!=b||n!=m{return fmt.Errorf("Repository readback mismatch: %s",t)}}else{mode:="0644";if t=="/supervisor/apps.json"||t=="/supervisor/store.json"{mode="0600"};if e=p.checkFile(t,files[t],mode);e!=nil{return e}}
 }
 return nil
}
func nativePrepare(ctx context.Context,r Request,output,tools string)(Receipt,error){
 start:=time.Now();var receipt Receipt
 var e error;output,e=filepath.Abs(output);if e!=nil{return receipt,e};r.Raw,e=filepath.Abs(r.Raw);if e!=nil{return receipt,e};r.Cache,e=filepath.Abs(r.Cache);if e!=nil{return receipt,e}
 if e:=validateProfile(r.Profile);e!=nil{return receipt,e};if e:=verifyTools(tools);e!=nil{return receipt,e}
 if _,e:=os.Stat(output);!os.IsNotExist(e){return receipt,errors.New("Output must be a new file")}
 actual,_,e:=hashFile(r.Raw);if e!=nil{return receipt,e};if actual!=r.Profile.OfficialHash{return receipt,errors.New("Official image hash mismatch")}
 work,e:=os.MkdirTemp(filepath.Dir(output),"native-");if e!=nil{return receipt,e};defer os.RemoveAll(work)
 p:=preparer{ctx:ctx,tools:tools,work:work,output:output}
 success:=false;defer func(){if !success{os.Remove(output)}}()
 src,e:=os.Open(r.Raw);if e!=nil{return receipt,e};defer src.Close();head:=make([]byte,34*512);if _,e=io.ReadFull(src,head);e!=nil{return receipt,e}
 first:=binary.LittleEndian.Uint64(head[1024+7*128+32:]);last:=binary.LittleEndian.Uint64(head[1024+7*128+40:]);info,e:=src.Stat();if e!=nil{return receipt,e}
 if first<34||last<first||last>=uint64(info.Size()/512)||last-first>16*1024*1024*1024/512{return receipt,errors.New("Invalid data partition bounds")}
 out,e:=os.OpenFile(output,os.O_CREATE|os.O_EXCL|os.O_RDWR,0600);if e!=nil{return receipt,e}
 if _,e=src.Seek(int64(first*512),0);e==nil{_,e=io.CopyN(out,src,int64((last-first+1)*512))};if e==nil{e=out.Truncate(2*1024*1024*1024)};out.Close();if e!=nil{return receipt,e}
 progress("prepare-connectivity",0,nil)
 if _,e=p.tool("e2fsck","-fn",output);e!=nil{return receipt,e};if _,e=p.tool("resize2fs",output);e!=nil{return receipt,e}
 store:=filepath.Join(work,"store");meta:=filepath.Join(store,"io.containerd.metadata.v1.bolt/meta.db");if e=os.MkdirAll(filepath.Dir(meta),0700);e!=nil{return receipt,e}
 if e=p.dump("/docker/containerd/daemon/io.containerd.metadata.v1.bolt/meta.db",meta);e!=nil{return receipt,e}
 progress("download-connectivity",0,nil)
 count,e:=registerImage(ctx,store,r.Cache,r.Profile);if e!=nil{return receipt,e};receipt.ExistingImages=count
 progress("register-connectivity",0,nil)
 repo,e:=checkout(ctx,work,r.Profile);if e!=nil{return receipt,e}
 files:=map[string]string{};dirs:=[]string{"/supervisor/apps","/supervisor/apps/data","/supervisor/apps/data/"+slug,"/supervisor/apps/git","/supervisor/apps/git/157e89e9"}
 addTree:=func(root,target string,includeDirs bool)error{return filepath.WalkDir(root,func(path string,d os.DirEntry,e error)error{if e!=nil{return e};if d.Type()&os.ModeSymlink!=0{return errors.New("Unexpected filesystem link")};rel,e:=filepath.Rel(root,path);if e!=nil{return e};if rel=="."{return nil};dest:=target+"/"+filepath.ToSlash(rel);if strings.ContainsAny(dest," \t\r\n\""){return errors.New("Unsupported image path")};if d.IsDir(){if includeDirs{dirs=append(dirs,dest)};return nil};files[dest]=path;return nil})}
 if e=addTree(store,"/docker/containerd/daemon",false);e!=nil{return receipt,e};if e=addTree(repo,"/supervisor/apps/git/157e89e9",true);e!=nil{return receipt,e}
 // No tokens, options.json, containers, machine identity or first-boot hooks.
 for name,data:=range map[string][]byte{"apps":r.Profile.State,"store":[]byte(`{"repositories":["`+repoURL+`"]}`)} {path:=filepath.Join(work,name+".json");if e=os.WriteFile(path,data,0600);e!=nil{return receipt,e};files["/supervisor/"+name+".json"]=path}
 if e=p.inject(files,dirs);e!=nil{return receipt,e}
 if _,e=p.tool("e2fsck","-fn",output);e!=nil{return receipt,e};if _,e=p.tool("resize2fs","-M",output);e!=nil{return receipt,e}
 f,e:=os.OpenFile(output,os.O_RDWR,0);if e!=nil{return receipt,e};sb:=make([]byte,1024);_,e=f.ReadAt(sb,1024);if e!=nil{f.Close();return receipt,e}
 blocks:=uint64(binary.LittleEndian.Uint32(sb[4:]))|uint64(binary.LittleEndian.Uint32(sb[0x150:]))<<32;block:=uint64(1024)<<binary.LittleEndian.Uint32(sb[24:]);size:=blocks*block
 if binary.LittleEndian.Uint16(sb[56:])!=0xef53||string(sb[120:131])!="hassos-data"||block>65536||size<2048||size>2*1024*1024*1024 {f.Close();return receipt,errors.New("Invalid resulting filesystem")}
 e=f.Truncate(int64(size));if e==nil{e=f.Sync()};f.Close();if e!=nil{return receipt,e};if _,e=p.tool("e2fsck","-fn",output);e!=nil{return receipt,e}
 receipt.SHA256,receipt.Bytes,e=hashFile(output);receipt.Seconds=time.Since(start).Seconds();if e!=nil{return receipt,e};success=true;return receipt,nil
}
func main(){
 if len(os.Args)!=4||os.Args[1]!="prepare"{fmt.Fprintln(os.Stderr,"Expected prepare <request.json> <new.ext4>");os.Exit(1)}
 b,e:=os.ReadFile(os.Args[2]);if e!=nil||len(b)>1024*1024{fmt.Fprintln(os.Stderr,"Invalid request");os.Exit(1)};var r Request;if e=json.Unmarshal(b,&r);e!=nil{fmt.Fprintln(os.Stderr,e);os.Exit(1)}
 self,e:=os.Executable();if e!=nil{os.Exit(1)};ctx,cancel:=context.WithTimeout(context.Background(),30*time.Minute);defer cancel()
 result,e:=nativePrepare(ctx,r,os.Args[3],filepath.Dir(self));if e!=nil{fmt.Fprintln(os.Stderr,e);os.Exit(1)};_ =json.NewEncoder(os.Stdout).Encode(result)
}

// No credentials are sent across redirects. TLS and download bounds stay enabled.
func client()*http.Client{return &http.Client{Timeout:2*time.Minute,CheckRedirect:func(r *http.Request,via []*http.Request)error{if len(via)>8||r.URL.Scheme!="https"{return errors.New("Invalid redirect")};r.Header.Del("Authorization");return nil}}}
