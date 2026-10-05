package main

import (
 "context"
 "encoding/json"
 "os"
 "path/filepath"
 "strings"
 "testing"
 "time"

 git "github.com/go-git/go-git/v5"
 gitconfig "github.com/go-git/go-git/v5/config"
 "github.com/go-git/go-git/v5/plumbing"
 "github.com/go-git/go-git/v5/plumbing/object"

 "github.com/containerd/containerd/v2/core/content"
 "github.com/containerd/containerd/v2/core/images"
 "github.com/containerd/containerd/v2/core/metadata"
 "github.com/containerd/containerd/v2/pkg/namespaces"
 "github.com/containerd/containerd/v2/plugins/content/local"
 "github.com/opencontainers/go-digest"
 "github.com/opencontainers/image-spec/specs-go"
 ocispec "github.com/opencontainers/image-spec/specs-go/v1"
 bolt "go.etcd.io/bbolt"
)

func stateProfile()Profile{
 state:=map[string]any{"system":map[string]any{slug:map[string]any{"version":"0.5.6","slug":"k11c_connectivity","repository":"157e89e9","startup":"system","arch":[]string{"aarch64"},"image":imageName}},"user":map[string]any{slug:map[string]any{"version":"0.5.6","image":imageName,"options":map[string]any{"enabled":true},"boot":"auto","protected":false}}}
 b,_:=json.Marshal(state)
 return Profile{HAOS:"18.3",Kernel:"6.18.52-haos",OfficialHash:strings.Repeat("a",64),Version:"0.5.6",Digest:imageName+"@sha256:"+strings.Repeat("b",64),ImageID:"sha256:"+strings.Repeat("c",64),Commit:strings.Repeat("d",40),ConfigHash:strings.Repeat("e",64),StateHash:hashBytes(b),State:b,Containerd:"2.3.4",Verified:true}
}
func TestProfile(t *testing.T){
 p:=stateProfile();if e:=validateProfile(p);e!=nil{t.Fatal(e)}
 for i:=0;i<7;i++{bad:=p;switch i{case 0:bad.Verified=false;case 1:bad.Containerd="1.7";case 2:bad.OfficialHash="bad";case 3:bad.Commit="../";case 4:bad.StateHash=strings.Repeat("0",64);case 5:bad.Digest="other.io/image@sha256:"+strings.Repeat("a",64);default:bad.State=[]byte(strings.Replace(string(p.State),`"enabled":true`,`"enabled":false`,1));bad.StateHash=hashBytes(bad.State)};if validateProfile(bad)==nil{t.Fatalf("Invalid profile passed %d",i)}}
}
func imageFixture(t *testing.T)(string,string,Profile){
 t.Helper();root:=t.TempDir();cache:=t.TempDir();ctx:=namespaces.WithNamespace(context.Background(),"moby")
 db,e:=bolt.Open(filepath.Join(root,"meta.tmp"),0600,nil);if e!=nil{t.Fatal(e)}
 backend,e:=local.NewStore(filepath.Join(root,"initial"));if e!=nil{t.Fatal(e)};md:=metadata.NewDB(db,backend,nil);if e=md.Init(ctx);e!=nil{t.Fatal(e)}
 _,e=metadata.NewImageStore(md).Create(ctx,images.Image{Name:"official/base:fixture",Target:ocispec.Descriptor{Digest:digest.FromString("untouched base"),Size:20,MediaType:ocispec.MediaTypeImageManifest}});if e!=nil{t.Fatal(e)};db.Close()
 os.MkdirAll(filepath.Join(root,"io.containerd.metadata.v1.bolt"),0700);if e=os.Rename(filepath.Join(root,"meta.tmp"),filepath.Join(root,"io.containerd.metadata.v1.bolt/meta.db"));e!=nil{t.Fatal(e)}
 layer:=[]byte("fixture compressed content")
 config:=ocispec.Image{Platform:ocispec.Platform{OS:"linux",Architecture:"arm64"},RootFS:ocispec.RootFS{Type:"layers",DiffIDs:[]digest.Digest{digest.FromBytes(layer)}},Config:ocispec.ImageConfig{Labels:map[string]string{"io.hass.version":"0.5.6","io.hass.arch":"aarch64"}}}
 cb,_:=json.Marshal(config)
 m:=ocispec.Manifest{Versioned:specs.Versioned{SchemaVersion:2},MediaType:ocispec.MediaTypeImageManifest,Config:ocispec.Descriptor{MediaType:ocispec.MediaTypeImageConfig,Digest:digest.FromBytes(cb),Size:int64(len(cb))},Layers:[]ocispec.Descriptor{{MediaType:ocispec.MediaTypeImageLayerGzip,Digest:digest.FromBytes(layer),Size:int64(len(layer))}}}
 mb,_:=json.Marshal(m);for _,b:=range [][]byte{mb,cb,layer}{if e=os.WriteFile(filepath.Join(cache,hashBytes(b)),b,0600);e!=nil{t.Fatal(e)}}
 p:=stateProfile();p.Digest=imageName+"@"+digest.FromBytes(mb).String();p.ImageID=m.Config.Digest.String();return root,cache,p
}
func TestRegistration(t *testing.T){root,cache,p:=imageFixture(t);ctx,cancel:=context.WithTimeout(context.Background(),5*time.Second);defer cancel();n,e:=registerImage(ctx,root,cache,p);if e!=nil{t.Fatal(e)};if n!=1{t.Fatal("Official image lost")}}
func TestWrongImageID(t *testing.T){root,cache,p:=imageFixture(t);p.ImageID="sha256:"+strings.Repeat("e",64);if _,e:=registerImage(context.Background(),root,cache,p);e==nil{t.Fatal("Wrong image identity accepted")}}
func TestPlatform(t *testing.T){
 root,cache,p:=imageFixture(t);ctx:=namespaces.WithNamespace(context.Background(),"moby");_,e:=registerImage(ctx,root,cache,p);if e!=nil{t.Fatal(e)}
 db,e:=bolt.Open(filepath.Join(root,"io.containerd.metadata.v1.bolt/meta.db"),0600,nil);if e!=nil{t.Fatal(e)};defer db.Close();localStore,_:=local.NewStore(filepath.Join(root,"io.containerd.content.v1.content"));cs:=metadata.NewDB(db,localStore,nil).ContentStore()
 cb,_:=json.Marshal(ocispec.Image{Platform:ocispec.Platform{OS:"linux",Architecture:"amd64"},RootFS:ocispec.RootFS{DiffIDs:[]digest.Digest{digest.FromString("layer")}},Config:ocispec.ImageConfig{Labels:map[string]string{"io.hass.version":p.Version,"io.hass.arch":"aarch64"}}})
 c:=ocispec.Descriptor{MediaType:ocispec.MediaTypeImageConfig,Digest:digest.FromBytes(cb),Size:int64(len(cb))};if e=content.WriteBlob(ctx,cs,"wrong-config",strings.NewReader(string(cb)),c);e!=nil{t.Fatal(e)}
 m:=ocispec.Manifest{Versioned:specs.Versioned{SchemaVersion:2},Config:c,Layers:[]ocispec.Descriptor{{Digest:digest.FromString("layer")}}};mb,_:=json.Marshal(m);target:=ocispec.Descriptor{MediaType:ocispec.MediaTypeImageManifest,Digest:digest.FromBytes(mb),Size:int64(len(mb))};content.WriteBlob(ctx,cs,"wrong-manifest",strings.NewReader(string(mb)),target);p.ImageID=c.Digest.String()
 if validateImage(ctx,cs,target,p)==nil{t.Fatal("Wrong platform accepted")}
}
func TestToolIntegrity(t *testing.T){
 dir:=t.TempDir();hashes:=map[string]string{};for _,n:=range []string{"debugfs.exe","e2fsck.exe","resize2fs.exe","cygwin1.dll"}{os.WriteFile(filepath.Join(dir,n),[]byte(n),0600);hashes[n]=hashBytes([]byte(n))};b,_:=json.Marshal(hashes);os.WriteFile(filepath.Join(dir,"tools.json"),b,0600)
 original:=toolsManifestHash;defer func(){toolsManifestHash=original}();toolsManifestHash=hashBytes(b)
 if e:=verifyTools(dir);e!=nil{t.Fatal(e)};os.WriteFile(filepath.Join(dir,"debugfs.exe"),[]byte("changed"),0600);if verifyTools(dir)==nil{t.Fatal("Changed executable accepted")}
 os.WriteFile(filepath.Join(dir,"debugfs.exe"),[]byte("debugfs.exe"),0600);os.WriteFile(filepath.Join(dir,"unexpected.dll"),[]byte("unapproved"),0600);if verifyTools(dir)==nil{t.Fatal("DLL search injection accepted")}
}
func TestRepositoryIdentity(t *testing.T){
 root:=t.TempDir();p:=stateProfile();repo,e:=git.PlainInitWithOptions(root,&git.PlainInitOptions{InitOptions:git.InitOptions{DefaultBranch:plumbing.NewBranchReferenceName("main")}});if e!=nil{t.Fatal(e)}
 app:=filepath.Join(root,"k11c_connectivity");if e=os.Mkdir(app,0755);e!=nil{t.Fatal(e)}
 config:=[]byte("configuration fixture");p.ConfigHash=hashBytes(config);os.WriteFile(filepath.Join(app,"config.yaml"),config,0644)
 release,_:=json.Marshal(map[string]any{"version":p.Version,"image_digest":p.Digest,"image_id":p.ImageID,"haos":map[string]any{p.HAOS:map[string]string{"kernel":p.Kernel}}});os.WriteFile(filepath.Join(app,"RELEASE.json"),release,0644)
 tree,e:=repo.Worktree();if e!=nil{t.Fatal(e)};tree.Add("k11c_connectivity/config.yaml");tree.Add("k11c_connectivity/RELEASE.json")
 commit,e:=tree.Commit("fixture",&git.CommitOptions{Author:&object.Signature{Name:"Test",Email:"test@invalid",When:time.Unix(1,0)}});if e!=nil{t.Fatal(e)};p.Commit=commit.String()
 repo.CreateRemote(&gitconfig.RemoteConfig{Name:"origin",URLs:[]string{repoURL+".git"}})
 cfg,_:=repo.Config();cfg.Branches["main"]=&gitconfig.Branch{Name:"main",Remote:"origin",Merge:plumbing.NewBranchReferenceName("main")};repo.SetConfig(cfg)
 if e=validateRepository(root,p);e!=nil{t.Fatal(e)}
 for _,field:=range []string{"commit","config","kernel","image","version"}{bad:=p;switch field{case "commit":bad.Commit=strings.Repeat("f",40);case "config":bad.ConfigHash=strings.Repeat("0",64);case "kernel":bad.Kernel="other-haos";case "image":bad.ImageID="sha256:"+strings.Repeat("0",64);case "version":bad.Version="0.0.0"};if validateRepository(root,bad)==nil{t.Fatal("Wrong repository identity accepted: "+field)}}
 cfg.Branches["main"].Remote="other";repo.SetConfig(cfg);if validateRepository(root,p)==nil{t.Fatal("Non-tracking checkout accepted")}
}
