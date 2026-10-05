package main

import (
 "context"
 "encoding/json"
 "errors"
 "fmt"
 "io"
 "os"
 "path/filepath"
 "strings"
 "time"

 "github.com/containerd/containerd/v2/core/content"
 "github.com/containerd/containerd/v2/core/images"
 "github.com/containerd/containerd/v2/core/metadata"
 "github.com/containerd/containerd/v2/core/remotes/docker"
 "github.com/containerd/containerd/v2/pkg/namespaces"
 "github.com/containerd/containerd/v2/plugins/content/local"
 "github.com/opencontainers/go-digest"
 ocispec "github.com/opencontainers/image-spec/specs-go/v1"
 bolt "go.etcd.io/bbolt"
)

func descriptor(d ocispec.Descriptor)error{if d.Digest.Algorithm()!=digest.SHA256||d.Digest.Validate()!=nil||d.Size<=0||d.Size>2*1024*1024*1024||len(d.URLs)!=0{return errors.New("Invalid image descriptor")};return nil}
func registerImage(ctx context.Context,root,cache string,p Profile)(int,error){
 path:=filepath.Join(root,"io.containerd.metadata.v1.bolt/meta.db");if _,e:=os.Stat(path);e!=nil{return 0,e}
 db,e:=bolt.Open(path,0600,&bolt.Options{Timeout:time.Second});if e!=nil{return 0,e};defer db.Close()
 backend,e:=local.NewStore(filepath.Join(root,"io.containerd.content.v1.content"));if e!=nil{return 0,e}
 md:=metadata.NewDB(db,backend,nil) // No Init/migration/GC of official HAOS metadata.
 ctx=namespaces.WithNamespace(ctx,"moby");store:=metadata.NewImageStore(md)
 before,e:=store.List(ctx);if e!=nil{return 0,e};if len(before)==0{return 0,errors.New("Missing official HAOS images")}
 cs:=md.ContentStore()
 manifestDigest:=digest.Digest(strings.TrimPrefix(p.Digest,imageName+"@"))
 resolver:=docker.NewResolver(docker.ResolverOptions{Client:client()})
 // Pinned manifest cache is self-authenticating and never bypasses graph checks.
 cached:=filepath.Join(cache,manifestDigest.Encoded())
 var target ocispec.Descriptor
 if raw,e:=os.ReadFile(cached);e==nil&&len(raw)<4*1024*1024&&hashBytes(raw)==manifestDigest.Encoded(){
  var m ocispec.Manifest;if e=json.Unmarshal(raw,&m);e!=nil{return 0,e};target=ocispec.Descriptor{Digest:manifestDigest,Size:int64(len(raw)),MediaType:m.MediaType}
 }else{_,target,e=resolver.Resolve(ctx,p.Digest);if e!=nil{return 0,e}}
 if target.Digest!=manifestDigest||!(target.MediaType==ocispec.MediaTypeImageManifest||target.MediaType==images.MediaTypeDockerSchema2Manifest){return 0,errors.New("Expected the pinned single-platform manifest")}
 fetcher,e:=resolver.Fetcher(ctx,p.Digest);if e!=nil{return 0,e}
 if e=os.MkdirAll(cache,0700);e!=nil{return 0,e}
 var downloaded int64
 fetch:=func(ctx context.Context,d ocispec.Descriptor)([]ocispec.Descriptor,error){
  if e:=descriptor(d);e!=nil{return nil,e}
  path:=filepath.Join(cache,d.Digest.Encoded());h,n,cacheError:=hashFile(path)
  if cacheError!=nil||h!=d.Digest.Encoded()||n!=d.Size {
   reader,e:=fetcher.Fetch(ctx,d);if e!=nil{return nil,e};defer reader.Close()
   temp,e:=os.CreateTemp(cache,"blob-");if e!=nil{return nil,e};tempName:=temp.Name();defer os.Remove(tempName)
   n,e:=io.Copy(temp,io.LimitReader(reader,d.Size+1));temp.Close();if e!=nil{return nil,e}
   h,_,e=hashFile(tempName);if e!=nil{return nil,e};if n!=d.Size||h!=d.Digest.Encoded(){return nil,errors.New("Connectivity blob hash/size mismatch")}
   if e=os.Remove(path);e!=nil&&!os.IsNotExist(e){return nil,e};if e=os.Rename(tempName,path);e!=nil{return nil,e}
  }
  f,e:=os.Open(path);if e!=nil{return nil,e};defer f.Close()
  if e=content.WriteBlob(ctx,cs,"k11c-"+d.Digest.Encoded(),f,d);e!=nil{return nil,e}
  downloaded+=d.Size;progress("download-connectivity",downloaded,nil)
  return images.Children(ctx,cs,d)
 }
 handler:=images.SetChildrenLabels(cs,images.HandlerFunc(fetch))
 if e=images.WalkNotEmpty(ctx,handler,target);e!=nil{return 0,e}
 if e=validateImage(ctx,cs,target,p);e!=nil{return 0,e}
 img,e:=store.Create(ctx,images.Image{Name:imageName+":"+p.Version,Target:target});if e!=nil{return 0,e}
 if img.Target.Digest!=manifestDigest{return 0,errors.New("Registered image changed")}
 // GC relationships must be present, including on cached content.
 if e=auditGraph(ctx,cs,target);e!=nil{return 0,e}
 after,e:=store.List(ctx);if e!=nil{return 0,e};if len(after)!=len(before)+1{return 0,errors.New("Unexpected image count")}
 for _,old:=range before{current,e:=store.Get(ctx,old.Name);if e!=nil||current.Target.Digest!=old.Target.Digest{return 0,fmt.Errorf("Official image changed: %s",old.Name)}}
 return len(before),nil
}
func validateImage(ctx context.Context,cs content.Store,target ocispec.Descriptor,p Profile)error{
 raw,e:=content.ReadBlob(ctx,cs,target);if e!=nil{return e};var m ocispec.Manifest;if e=json.Unmarshal(raw,&m);e!=nil{return e}
 if m.SchemaVersion!=2||string(m.Config.Digest)!=p.ImageID||len(m.Layers)==0||len(m.Layers)>128{return errors.New("Wrong Connectivity image identity")}
 config,e:=content.ReadBlob(ctx,cs,m.Config);if e!=nil{return e};var c ocispec.Image;if e=json.Unmarshal(config,&c);e!=nil{return e}
 if c.OS!="linux"||c.Architecture!="arm64"||len(c.RootFS.DiffIDs)!=len(m.Layers)||c.Config.Labels["io.hass.version"]!=p.Version||c.Config.Labels["io.hass.arch"]!="aarch64" {return errors.New("Wrong app platform/version")}
 return nil
}
func auditGraph(ctx context.Context,cs content.Store,target ocispec.Descriptor)error{
 return images.WalkNotEmpty(ctx,images.HandlerFunc(func(ctx context.Context,d ocispec.Descriptor)([]ocispec.Descriptor,error){
  children,e:=images.Children(ctx,cs,d);if e!=nil{return nil,e};info,e:=cs.Info(ctx,d.Digest);if e!=nil{return nil,e}
  for _,child:=range children{found:=false;for k,v:=range info.Labels{if strings.HasPrefix(k,"containerd.io/gc.ref.content")&&v==child.Digest.String(){found=true}}
   if !found{return nil,errors.New("Missing image GC relationship")}}
  return children,nil
 }),target)
}
