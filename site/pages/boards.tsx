import Layout from '../components/Layout';

export const config={unstable_runtimeJS:false};

export default function BoardsPage(){
 return <Layout>
<section id="boards">
 <div className="wrap">
  <h1 data-t="boardsTitle">Supported boards</h1>
  <p className="lead" data-t="boardsLead">One firmware per family. Boards marked experimental still need reports from real hardware: they are welcome.</p>
  <ul className="boards" id="board-list"></ul>
  <div id="board-comparison"></div>
 </div>
</section>
 </Layout>;
}
