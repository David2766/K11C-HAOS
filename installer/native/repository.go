package main

import (
 "context"
 "encoding/json"
 "errors"
 "os"
 "path/filepath"

 git "github.com/go-git/go-git/v5"
 "github.com/go-git/go-git/v5/config"
 "github.com/go-git/go-git/v5/plumbing"
)

func checkout(ctx context.Context,work string,p Profile)(string,error){
 path:=filepath.Join(work,"repository")
 repo,e:=git.PlainInit(path,false);if e!=nil{return "",e}
 if _,e=repo.CreateRemote(&config.RemoteConfig{Name:"origin",URLs:[]string{repoURL+".git"},Fetch:[]config.RefSpec{"+refs/heads/main:refs/remotes/origin/main"}});e!=nil{return "",e}
 // Fetch only the selected commit. No compiler, VM or external Git executable.
 e=repo.FetchContext(ctx,&git.FetchOptions{RemoteName:"origin",Depth:1,RefSpecs:[]config.RefSpec{config.RefSpec("+"+p.Commit+":refs/remotes/origin/main")}})
 if e!=nil&&e!=git.NoErrAlreadyUpToDate{return "",e}
 branch:=plumbing.NewBranchReferenceName("main");if e=repo.Storer.SetReference(plumbing.NewHashReference(branch,plumbing.NewHash(p.Commit)));e!=nil{return "",e}
 tree,e:=repo.Worktree();if e!=nil{return "",e};if e=tree.Checkout(&git.CheckoutOptions{Branch:branch});e!=nil{return "",e}
 cfg,e:=repo.Config();if e!=nil{return "",e};cfg.Branches["main"]=&config.Branch{Name:"main",Remote:"origin",Merge:branch};if e=repo.SetConfig(cfg);e!=nil{return "",e}
 if e=validateRepository(path,p);e!=nil{return "",e};return path,nil
}
func validateRepository(path string,p Profile)error{
 repo,e:=git.PlainOpen(path);if e!=nil{return e};head,e:=repo.Head();if e!=nil{return e}
 cfg,e:=repo.Config();if e!=nil{return e};branch:=cfg.Branches["main"];origin:=cfg.Remotes["origin"]
 if head.Name()!=plumbing.NewBranchReferenceName("main")||head.Hash().String()!=p.Commit||branch==nil||branch.Remote!="origin"||branch.Merge!=head.Name()||origin==nil||len(origin.URLs)!=1||origin.URLs[0]!=repoURL+".git" {return errors.New("Invalid ordinary repository checkout")}
 config,e:=os.ReadFile(filepath.Join(path,"k11c_connectivity/config.yaml"));if e!=nil{return e};if hashBytes(config)!=p.ConfigHash{return errors.New("App configuration changed")}
 b,e:=os.ReadFile(filepath.Join(path,"k11c_connectivity/RELEASE.json"));if e!=nil{return e}
 var release struct{Version string `json:"version"`;Digest string `json:"image_digest"`;ID string `json:"image_id"`;HAOS map[string]struct{Kernel string `json:"kernel"`} `json:"haos"`}
 if e=json.Unmarshal(b,&release);e!=nil{return e};if release.Version!=p.Version||release.Digest!=p.Digest||release.ID!=p.ImageID||release.HAOS[p.HAOS].Kernel!=p.Kernel{return errors.New("Connectivity/HAOS release mismatch")}
 return nil
}
